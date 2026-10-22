use super::*;

pub(super) fn key<'a>(
    container: &'a Container,
    chunk: &crate::maya_binary::container::Chunk,
) -> Result<&'a [u8], BinaryError> {
    let range = chunk.payload_range();
    let payload = &container.source()[range.clone()];
    let end = payload
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| fail(ErrorKind::Malformed, range.start))?;
    Ok(&payload[..end])
}

fn normalized(key: &[u8], pattern: &[&[u8]]) -> bool {
    let mut pieces = key.split(|byte| *byte == b'.');
    for expected in pattern {
        let Some(piece) = pieces.next() else {
            return false;
        };
        let root = piece
            .iter()
            .position(|byte| *byte == b'[')
            .unwrap_or(piece.len());
        if &piece[..root] != *expected {
            return false;
        }
        if root < piece.len() {
            let Some(indices) = piece[root + 1..].strip_suffix(b"]") else {
                return false;
            };
            let mut bounds = indices.split(|byte| *byte == b':');
            let Some(first) = bounds.next() else {
                return false;
            };
            let parse = |part: &[u8]| {
                std::str::from_utf8(part)
                    .ok()
                    .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|s| s.parse::<usize>().ok())
            };
            let Some(first) = parse(first) else {
                return false;
            };
            if let Some(second) = bounds.next()
                && (parse(second).is_none_or(|second| second < first) || bounds.next().is_some())
            {
                return false;
            }
        }
    }
    pieces.next().is_none()
}

pub(super) fn inspect(
    container: &Container,
    facts: &WireFacts<'_>,
    bindings: &[Binding],
    names: &[usize],
    owners: &Owners,
    budget: &mut Budget,
) -> Result<usize, BinaryError> {
    let definitions = custom::Definitions::inspect(container, bindings, budget)?;
    let mut offset_count = 0usize;
    for binding in bindings {
        let mut hint = None;
        let mut offsets = 0usize;
        for chunk in children(container, binding.group) {
            if matches!(&chunk.tag(), b"CREA" | b"ATTR") {
                continue;
            }
            if chunk.tag() == *b"MESH" {
                if binding.transform.is_some() || !matches!(key(container, chunk)?, b"o" | b"ci") {
                    return Err(fail(ErrorKind::Unsupported, chunk.payload_range().start));
                }
                continue;
            }
            let key = key(container, chunk)?;
            let at = chunk.payload_range().start;
            if binding.transform.is_some() {
                if chunk.tag() == *b"DBL3" && field(key).is_some()
                    || chunk.tag() == *b"DBLE" && key == b"v"
                {
                    continue;
                }
                if definitions.state(binding.group, key, chunk)? {
                    continue;
                }
                return Err(fail(ErrorKind::Unsupported, at));
            }
            if chunk.tag() == *b"FLT3" {
                let (_, range) = geometry::tweak(container, chunk)?;
                offsets = offsets.checked_add(range.len() / 12).ok_or_else(resource)?;
                continue;
            }
            if chunk.tag() == *b"FLGS" && key == b"pt" {
                let payload = &container.source()[chunk.payload_range()];
                if payload.len() != 8 || payload[3] != 0x28 || hint.is_some() {
                    return Err(fail(ErrorKind::Unsupported, at));
                }
                hint = Some(u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize);
                continue;
            }
            if chunk.tag() == *b"FLGS" && key == b"v" {
                if &container.source()[chunk.payload_range()] != b"v\0\x20" {
                    return Err(fail(ErrorKind::Unsupported, at));
                }
                continue;
            }
            let metadata = match &chunk.tag() {
                b"DBLE" => matches!(key, b"vir" | b"vif"),
                b"DBL2" => key == b"pv",
                b"FLGS" => {
                    normalized(key, &[b"iog", b"og"])
                        || normalized(key, &[b"uvst", b"uvsp"])
                        || key == b"gtag"
                }
                b"FLT2" => normalized(key, &[b"uvst", b"uvsp"]),
                b"STR " => {
                    matches!(key, b"cuvs" | b"dcc")
                        || normalized(key, &[b"uvst", b"uvsn"])
                        || normalized(key, &[b"gtag", b"gtagnm"])
                }
                b"CMP#" => {
                    normalized(key, &[b"iog", b"og", b"gcl"])
                        || normalized(key, &[b"gtag", b"gtagcmp"])
                }
                b"CMPD" => normalized(key, &[b"covm"]) || normalized(key, &[b"cdvm"]),
                _ => false,
            };
            if !metadata {
                return Err(fail(ErrorKind::Unsupported, at));
            }
        }
        if hint.is_some_and(|hint| hint != offsets) || offsets > 0 && hint.is_none() {
            return Err(fail(
                ErrorKind::Malformed,
                container.chunk(binding.group).unwrap().header_range().start,
            ));
        }
        if offsets > 0 {
            let record = binding
                .mesh
                .and_then(|index| facts.records.get(index))
                .ok_or_else(|| fail(ErrorKind::Malformed, 0))?;
            let Data::Mesh(mesh) = &record.data else {
                unreachable!()
            };
            if &container.source()[mesh.key.clone()] != b"ci" {
                return Err(fail(ErrorKind::Unsupported, mesh.key.start));
            }
        }
        offset_count = offset_count.checked_add(offsets).ok_or_else(resource)?;
    }
    for record in &facts.records {
        if let Data::Selection(target) = &record.data {
            let target = &container.source()[target.clone()];
            if names
                .binary_search_by(|index| name(facts, &bindings[*index]).cmp(&target[1..]))
                .is_ok()
            {
                return Err(fail(
                    ErrorKind::Unsupported,
                    container.chunk(record.chunk).unwrap().payload_range().start,
                ));
            }
        }
        let Data::Connection(connection) = &record.data else {
            continue;
        };
        let resolve = |range: &Range<usize>| -> Result<(Option<usize>, &[u8]), BinaryError> {
            let (index, attribute) = owners.resolve(facts, range)?;
            Ok((index, &container.source()[attribute]))
        };
        let source = resolve(&connection.source)?;
        let destination = resolve(&connection.destination)?;
        for (range, incoming, endpoint, other) in [
            (&connection.source, false, source, destination),
            (&connection.destination, true, destination, source),
        ] {
            let Some(created_index) = endpoint.0 else {
                continue;
            };
            let Data::Create(create) = &facts.records[created_index].data else {
                unreachable!()
            };
            let owner = &container.source()[create.name.clone()];
            let attribute = endpoint.1;
            if let Ok(index) =
                names.binary_search_by(|index| name(facts, &bindings[*index]).cmp(owner))
            {
                let binding = &bindings[names[index]];
                let form = other
                    .0
                    .and_then(|index| container.chunk(facts.records[index].group))
                    .and_then(|chunk| chunk.form());
                let membership = if incoming {
                    binding.transform.is_none()
                        && (normalized(attribute, &[b"iog", b"og", b"gid"])
                            && form == Some(*b"GPID")
                            && other.1 == b"id"
                            || normalized(attribute, &[b"iog", b"og", b"gco"])
                                && matches!(form, Some(form) if form == *b"SHAD" || form == *b"OBST"))
                } else {
                    (normalized(attribute, &[b"iog"]) || normalized(attribute, &[b"iog", b"og"]))
                        && normalized(other.1, &[b"dsm"])
                        && (other.0.is_none()
                            || matches!(form, Some(form) if form == *b"SHAD" || form == *b"OBST"))
                };
                let custom = !incoming
                    && binding.transform.is_some()
                    && definitions.output(binding.group, attribute)
                    && form == Some(*b"PCUU")
                    && other.1 == b"i";
                if !membership && !custom {
                    return Err(fail(ErrorKind::Unsupported, range.start));
                }
            }
        }
    }
    Ok(offset_count)
}
