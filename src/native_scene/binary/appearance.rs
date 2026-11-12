use super::{
    BinaryError, ErrorKind,
    admission::Budget,
    binding::{Binding, Owners, children},
    fail, resource,
};
use crate::{
    maya_binary::{
        container::{Chunk, Container},
        semantic::{Data, WireFacts},
    },
    native_scene::{NativeId, NativeScene, appearance::*},
};
use std::{collections::HashMap, mem::size_of, ops::Range, sync::Arc};

#[cfg(test)]
mod tests;

const MAX_METADATA: usize = 8 * 1024 * 1024;
const MAX_STEPS: usize = 1_000_000;
const PLACEMENT: [(&[u8], &[u8]); 11] = [
    (b"o", b"uv"),
    (b"ofu", b"ofu"),
    (b"ofv", b"ofv"),
    (b"rf", b"rf"),
    (b"reu", b"reu"),
    (b"rev", b"rev"),
    (b"vt1", b"vt1"),
    (b"vt2", b"vt2"),
    (b"vt3", b"vt3"),
    (b"vc1", b"vc1"),
    (b"ofs", b"fs"),
];
const COLOR_LINKS: [(&[u8], &[u8]); 4] = [
    (b"cme", b"cme"),
    (b"cfe", b"cmcf"),
    (b"cfp", b"cmcp"),
    (b"wsn", b"ws"),
];

struct Meter<'a> {
    budget: &'a mut Budget,
    bytes: usize,
    steps: usize,
}

impl Meter<'_> {
    fn charge(&mut self, bytes: usize) -> Result<(), BinaryError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|sum| *sum <= MAX_METADATA)
            .ok_or_else(resource)?;
        Ok(())
    }
    fn step(&mut self, count: usize) -> Result<(), BinaryError> {
        self.steps = self
            .steps
            .checked_add(count)
            .filter(|sum| *sum <= MAX_STEPS)
            .ok_or_else(resource)?;
        Ok(())
    }
    /// Charges a hash map of `entries` pairs before it is built: the pair plus a word of
    /// control and load-factor slack per entry, doubled for the table's growth headroom.
    fn charge_map<K, V>(&mut self, entries: usize) -> Result<(), BinaryError> {
        self.charge(map_bytes::<K, V>(entries)?)
    }
    fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, BinaryError> {
        let bytes = count.checked_mul(size_of::<T>()).ok_or_else(resource)?;
        if self
            .bytes
            .checked_add(bytes)
            .is_none_or(|sum| sum > MAX_METADATA)
        {
            return Err(resource());
        }
        let vector = self.budget.vector::<T>(count)?;
        self.charge(
            vector
                .capacity()
                .checked_mul(size_of::<T>())
                .ok_or_else(resource)?,
        )?;
        Ok(vector)
    }
}

fn map_bytes<K, V>(entries: usize) -> Result<usize, BinaryError> {
    size_of::<(K, V)>()
        .checked_add(size_of::<usize>())
        .and_then(|entry| entry.checked_mul(2))
        .and_then(|entry| entry.checked_mul(entries))
        .ok_or_else(resource)
}

struct Link {
    chunk: usize,
    source: Option<usize>,
    destination: Option<usize>,
    source_key: Range<usize>,
    destination_key: Range<usize>,
    source_owner: Range<usize>,
    destination_owner: Range<usize>,
    mode: u8,
}

struct Graph<'a> {
    facts: &'a WireFacts<'a>,
    owners: &'a Owners,
    links: Vec<Link>,
    cyclic: bool,
}

fn form(facts: &WireFacts<'_>, record: usize) -> Option<[u8; 4]> {
    facts
        .records
        .get(record)
        .and_then(|record| facts.container.chunk(record.group))
        .and_then(Chunk::form)
}

fn surface_form(value: Option<[u8; 4]>) -> bool {
    matches!(value, Some(value) if matches!(&value, b"SHAD" | b"RPHO" | b"RTFT" | b"RPL2"))
}

fn message_link(facts: &WireFacts<'_>, link: &Link) -> bool {
    let Some(owner) = link.source else {
        return false;
    };
    let owner_form = form(facts, owner);
    let key = &facts.container.source()[link.source_key.clone()];
    surface_form(owner_form) && matches!(key, b"msg" | b"message")
        || owner_form == Some(*b"SHAD") && matches!(key, b"pa" | b"partition")
}

impl<'a> Graph<'a> {
    fn new(
        facts: &'a WireFacts<'a>,
        owners: &'a Owners,
        meter: &mut Meter<'_>,
    ) -> Result<Self, BinaryError> {
        meter.step(facts.records.len())?;
        let count = facts
            .records
            .iter()
            .filter(|record| matches!(record.data, Data::Connection(_)))
            .count();
        let mut links = meter.vector(count)?;
        for record in &facts.records {
            if let Data::Connection(connection) = &record.data {
                meter.step(2 * (usize::BITS - facts.records.len().leading_zeros()) as usize)?;
                let (source, source_key) = owners.resolve(facts, &connection.source)?;
                let (destination, destination_key) =
                    owners.resolve(facts, &connection.destination)?;
                let source_owner = connection.source.start..source_key.start - 1;
                let destination_owner = connection.destination.start..destination_key.start - 1;
                links.push(Link {
                    chunk: record.chunk,
                    source,
                    destination,
                    source_key,
                    destination_key,
                    source_owner,
                    destination_owner,
                    mode: connection.mode,
                });
            }
        }
        let mut indegree = meter.vector::<usize>(facts.records.len())?;
        indegree.resize(facts.records.len(), 0);
        let mut remaining = 0usize;
        meter.step(facts.records.len())?;
        for (index, record) in facts.records.iter().enumerate() {
            if matches!(record.data, Data::Create(_)) && surface_form(form(facts, index)) {
                remaining += 1;
            }
        }
        for link in &links {
            meter.step(1)?;
            if message_link(facts, link) {
                continue;
            }
            if let (Some(source), Some(destination)) = (link.source, link.destination)
                && surface_form(form(facts, source))
                && surface_form(form(facts, destination))
            {
                indegree[destination] =
                    indegree[destination].checked_add(1).ok_or_else(resource)?;
            }
        }
        let mut done = meter.vector::<bool>(facts.records.len())?;
        done.resize(facts.records.len(), false);
        loop {
            let mut progress = false;
            for index in 0..facts.records.len() {
                meter.step(1)?;
                if done[index]
                    || !matches!(facts.records[index].data, Data::Create(_))
                    || !surface_form(form(facts, index))
                    || indegree[index] != 0
                {
                    continue;
                }
                done[index] = true;
                remaining -= 1;
                progress = true;
                meter.step(links.len())?;
                for link in &links {
                    if message_link(facts, link) {
                        continue;
                    }
                    if link.source == Some(index)
                        && let Some(destination) = link.destination
                        && surface_form(form(facts, destination))
                    {
                        indegree[destination] =
                            indegree[destination].checked_sub(1).ok_or_else(resource)?;
                    }
                }
            }
            if !progress {
                break;
            }
        }
        Ok(Self {
            facts,
            owners,
            links,
            cyclic: remaining != 0,
        })
    }

    fn key(&self, range: &Range<usize>) -> &[u8] {
        &self.facts.container.source()[range.clone()]
    }

    fn incoming(
        &self,
        owner: usize,
        key: &[u8],
        meter: &mut Meter<'_>,
    ) -> Result<Result<usize, AppearanceIssue>, BinaryError> {
        meter.step(self.links.len())?;
        let mut found = None;
        for (index, link) in self.links.iter().enumerate() {
            if link.destination == Some(owner)
                && self.key(&link.destination_key) == key
                && found.replace(index).is_some()
            {
                return Ok(Err(AppearanceIssue::UnsupportedGraph));
            }
        }
        Ok(found.ok_or(AppearanceIssue::UnsupportedGraph))
    }

    fn span(&self, index: usize) -> SourceSpan {
        let chunk = self.facts.container.chunk(self.links[index].chunk).unwrap();
        SourceSpan {
            leaf: chunk.payload_range(),
            value: chunk.payload_range(),
        }
    }
}

struct Field {
    key: Range<usize>,
    value: Range<usize>,
    kind: u8,
}

fn field(container: &Container, chunk: &Chunk) -> Result<Field, BinaryError> {
    let payload = chunk.payload_range();
    let bytes = &container.source()[payload.clone()];
    let nul = bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| fail(ErrorKind::Malformed, payload.start))?;
    let kind = bytes
        .get(nul + 1)
        .copied()
        .ok_or_else(|| fail(ErrorKind::Malformed, payload.start))?;
    Ok(Field {
        key: payload.start..payload.start + nul,
        kind,
        value: payload.start + nul + 2..payload.end,
    })
}

fn span(chunk: &Chunk, value: Range<usize>) -> SourceSpan {
    SourceSpan {
        leaf: chunk.payload_range(),
        value,
    }
}

fn finite_words(source: &[u8], range: &Range<usize>, width: usize) -> bool {
    source[range.clone()].chunks_exact(width).all(|word| {
        if width == 4 {
            f32::from_be_bytes(word.try_into().unwrap()).is_finite()
        } else {
            f64::from_be_bytes(word.try_into().unwrap()).is_finite()
        }
    })
}

enum BindingFailure {
    Unavailable(AppearanceIssue),
    Fatal(BinaryError),
}
impl From<AppearanceIssue> for BindingFailure {
    fn from(issue: AppearanceIssue) -> Self {
        Self::Unavailable(issue)
    }
}
impl From<BinaryError> for BindingFailure {
    fn from(error: BinaryError) -> Self {
        Self::Fatal(error)
    }
}
type BindingResult<T> = Result<T, BindingFailure>;

fn input(
    graph: &Graph<'_>,
    destination: usize,
    key: &[u8],
    expected_form: [u8; 4],
    source_key: &[u8],
    mode: u8,
    meter: &mut Meter<'_>,
) -> BindingResult<usize> {
    let index = graph.incoming(destination, key, meter)??;
    let link = &graph.links[index];
    if link.mode != mode
        || graph.key(&link.source_key) != source_key
        || link
            .source
            .is_none_or(|owner| form(graph.facts, owner) != Some(expected_form))
    {
        return Err(AppearanceIssue::UnsupportedGraph.into());
    }
    Ok(index)
}

fn string_value<'a>(container: &'a Container, field: &Field) -> Result<&'a [u8], BinaryError> {
    let value = &container.source()[field.value.clone()];
    value
        .strip_suffix(&[0])
        .filter(|value| !value.contains(&0))
        .ok_or_else(|| fail(ErrorKind::Malformed, field.value.start))
}

fn range_key(key: &[u8], prefix: &[u8]) -> Option<(usize, usize)> {
    let range = key
        .strip_prefix(prefix)?
        .strip_prefix(b"[")?
        .strip_suffix(b"]")?;
    let colon = range.iter().position(|byte| *byte == b':')?;
    let number = |bytes: &[u8]| -> Option<usize> {
        if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(bytes).ok()?.parse().ok()
    };
    Some((number(&range[..colon])?, number(&range[colon + 1..])?))
}

fn check_owner_child(
    facts: &WireFacts<'_>,
    owner: usize,
    chunk: &Chunk,
) -> Result<(), BinaryError> {
    let creation = facts.container.chunk(facts.records[owner].chunk).unwrap();
    if chunk.header_range().start <= creation.header_range().start {
        return Err(fail(ErrorKind::Malformed, chunk.header_range().start));
    }
    Ok(())
}

fn preflight(
    graph: &Graph<'_>,
    faces_of: &FaceCounts,
    meter: &mut Meter<'_>,
) -> Result<(), BinaryError> {
    for (owner, record) in graph.facts.records.iter().enumerate() {
        meter.step(1)?;
        let selected_cm = matches!(&record.data, Data::Selection(target) if graph.key(target) == b":defaultColorMgtGlobals");
        let owner_form = form(graph.facts, owner);
        if !selected_cm
            && (!matches!(record.data, Data::Create(_))
                || !matches!(owner_form, Some(value) if matches!(&value, b"DMSH" | b"RPHO" | b"RTFT" | b"RPL2" | b"SHAD")))
        {
            continue;
        }
        let faces = if owner_form == Some(*b"DMSH") {
            meter.step(1)?;
            Some(faces_of.by_record(owner)?)
        } else {
            None
        };
        for chunk in children(graph.facts.container, record.group) {
            meter.step(1)?;
            if matches!(&chunk.tag(), b"CREA" | b"SLCT" | b"MESH") {
                continue;
            }
            if !matches!(
                &chunk.tag(),
                b"CMP#" | b"FLGS" | b"FLT2" | b"FLT3" | b"DBLE" | b"STR "
            ) {
                continue;
            }
            if !selected_cm {
                check_owner_child(graph.facts, owner, chunk)?;
            }
            let field = field(graph.facts.container, chunk)?;
            let key = graph.key(&field.key);
            if let Some(faces) = faces
                && chunk.tag() == *b"CMP#"
                && key == b"iog[0].og[0].gcl"
                && let Err(BindingFailure::Fatal(error)) =
                    component_bounds(graph.facts.container, chunk, &field, faces)
            {
                return Err(error);
            }
            let layout = (owner_form, chunk.tag(), key, field.kind);
            let expected = match layout {
                (Some(value), tag, b"uvst[0].uvsp", 0x28)
                    if value == *b"DMSH" && tag == *b"FLGS" =>
                {
                    Some(4)
                }
                (Some(value), tag, _, 0x20) if value == *b"DMSH" && tag == *b"FLT2" => {
                    if let Some((start, end)) = range_key(key, b"uvst[0].uvsp") {
                        Some(
                            end.checked_sub(start)
                                .and_then(|count| count.checked_add(1))
                                .and_then(|count| count.checked_mul(8))
                                .ok_or_else(|| fail(ErrorKind::Malformed, field.value.start))?,
                        )
                    } else {
                        None
                    }
                }
                (Some(value), tag, b"dc", 0x24) if value == *b"RPHO" && tag == *b"DBLE" => Some(8),
                (Some(value), tag, b"rfl", 0x20) if value == *b"RPHO" && tag == *b"DBLE" => Some(8),
                (
                    Some(value),
                    tag,
                    b"ihi" | b"isHistoricallyInteresting" | b"ro" | b"renderableOnlySet",
                    0x20,
                ) if value == *b"SHAD" && tag == *b"DBLE" => Some(8),
                (Some(value), tag, b"ambc" | b"sc", 0x20)
                    if value == *b"RPHO" && tag == *b"FLT3" =>
                {
                    Some(12)
                }
                (_, tag, b"cfe" | b"cmConfigFileEnabled", 0x20)
                    if selected_cm && tag == *b"DBLE" =>
                {
                    Some(8)
                }
                _ => None,
            };
            if expected.is_some_and(|length| length != field.value.len()) {
                return Err(fail(ErrorKind::Malformed, field.value.start));
            }
            let string = chunk.tag() == *b"STR "
                && match owner_form {
                    Some(value) if value == *b"DMSH" => {
                        matches!((key, field.kind), (b"uvst[0].uvsn", 0x24) | (b"cuvs", 0x20))
                    }
                    Some(value) if value == *b"RTFT" => {
                        matches!(key, b"ftn" | b"cs") && field.kind == 0x20
                    }
                    _ => {
                        selected_cm
                            && matches!(
                                key,
                                b"cfp" | b"cmConfigFilePath" | b"wsn" | b"workingSpaceName"
                            )
                            && field.kind == 0x20
                    }
                };
            if string {
                string_value(graph.facts.container, &field)?;
            }
        }
    }
    Ok(())
}

fn component_bounds(
    container: &Container,
    chunk: &Chunk,
    field: &Field,
    faces: usize,
) -> BindingResult<Range<usize>> {
    if field.kind != 0x20 {
        return Err(AppearanceIssue::UnsupportedSettings.into());
    }
    let bytes = &container.source()[field.value.clone()];
    if bytes.len() < 4 {
        return Err(fail(ErrorKind::Malformed, field.value.start).into());
    }
    if u32::from_be_bytes(bytes[..4].try_into().unwrap()) != 1 {
        return Err(AppearanceIssue::UnsupportedSettings.into());
    }
    if bytes.len() < 8 {
        return Err(fail(ErrorKind::Malformed, field.value.start).into());
    }
    if &bytes[4..8] != b"CMDF" {
        return Err(AppearanceIssue::UnsupportedSettings.into());
    }
    if bytes.len() < 12 {
        return Err(fail(ErrorKind::Malformed, field.value.start).into());
    }
    if u32::from_be_bytes(bytes[8..12].try_into().unwrap()) != 1 {
        return Err(AppearanceIssue::UnsupportedSettings.into());
    }
    if bytes.len() != 20 {
        return Err(fail(ErrorKind::Malformed, field.value.start).into());
    }
    let words = bytes.as_chunks::<4>().0;
    let start = u32::from_be_bytes(words[3]) as usize;
    let end = (u32::from_be_bytes(words[4]) as usize)
        .checked_add(1)
        .ok_or_else(resource)?;
    if start >= end || end > faces {
        return Err(fail(ErrorKind::Malformed, chunk.payload_range().start).into());
    }
    Ok(start..end)
}

fn component(
    container: &Container,
    chunk: &Chunk,
    field: &Field,
    faces: usize,
) -> BindingResult<Range<usize>> {
    let range = component_bounds(container, chunk, field, faces)?;
    if range != (0..faces) {
        return Err(AppearanceIssue::MissingMembership.into());
    }
    Ok(range)
}

fn uv_binding(
    container: &Container,
    facts: &WireFacts<'_>,
    owner: usize,
    mesh: &crate::maya_binary::semantic::mesh::Mesh,
    mesh_chunk: &Chunk,
    meter: &mut Meter<'_>,
) -> BindingResult<(SourceSpan, SourceSpan, SourceSpan, SourceSpan)> {
    let group = facts.records[owner].group;
    let mut explicit = None;
    let mut name = None;
    let mut active = None;
    let mut hint = None;
    for chunk in children(container, group) {
        meter.step(1)?;
        if !matches!(&chunk.tag(), b"FLT2" | b"STR " | b"FLGS") {
            continue;
        }
        check_owner_child(facts, owner, chunk)?;
        let field = field(container, chunk)?;
        let key = &container.source()[field.key.clone()];
        if chunk.tag() == *b"FLT2" && key.starts_with(b"uvst[") {
            let (start, end) =
                range_key(key, b"uvst[0].uvsp").ok_or(AppearanceIssue::UnsupportedSettings)?;
            let count = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(resource)?;
            if field.kind != 0x20 {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            if count.checked_mul(8) != Some(field.value.len()) {
                return Err(fail(ErrorKind::Malformed, field.value.start).into());
            }
            if start != 0
                || count.checked_mul(2) != Some(mesh.uv_points.count)
                || explicit.is_some()
            {
                return Err(AppearanceIssue::UvMismatch.into());
            }
            meter.step(count)?;
            if !finite_words(container.source(), &field.value, 4) {
                return Err(AppearanceIssue::UvMismatch.into());
            }
            explicit = Some(span(chunk, field.value));
        } else if chunk.tag() == *b"FLGS" && key == b"uvst[0].uvsp" {
            if field.kind != 0x28 {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            if field.value.len() != 4 {
                return Err(fail(ErrorKind::Malformed, field.value.start).into());
            }
            if hint
                .replace(
                    u32::from_be_bytes(container.source()[field.value].try_into().unwrap())
                        as usize,
                )
                .is_some()
            {
                return Err(AppearanceIssue::UvMismatch.into());
            }
        } else if chunk.tag() == *b"STR " && matches!(key, b"uvst[0].uvsn" | b"cuvs") {
            if field.kind != if key == b"cuvs" { 0x20 } else { 0x24 } {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            let value = string_value(container, &field)?;
            if value != &container.source()[mesh.uv_name.clone()] {
                return Err(AppearanceIssue::UvMismatch.into());
            }
            let target = if key == b"cuvs" {
                &mut active
            } else {
                &mut name
            };
            if target
                .replace(span(chunk, field.value.start..field.value.end - 1))
                .is_some()
            {
                return Err(AppearanceIssue::UvMismatch.into());
            }
        }
    }
    let explicit = explicit.ok_or(AppearanceIssue::UvMismatch)?;
    meter.step(mesh.uv_points.count)?;
    if hint != Some(mesh.uv_points.count / 2)
        || !finite_words(container.source(), &mesh.uv_points.range, 4)
        || container.source()[explicit.value.clone()]
            != container.source()[mesh.uv_points.range.clone()]
    {
        return Err(AppearanceIssue::UvMismatch.into());
    }
    Ok((
        explicit,
        name.ok_or(AppearanceIssue::UvMismatch)?,
        active.ok_or(AppearanceIssue::UvMismatch)?,
        span(mesh_chunk, mesh.uv_points.range.clone()),
    ))
}

fn color_inputs(
    graph: &Graph<'_>,
    file: usize,
    inputs: &mut Vec<InputEvidence>,
    meter: &mut Meter<'_>,
) -> BindingResult<[SourceSpan; 4]> {
    let mut links = meter.vector(4)?;
    for (source_key, destination_key) in COLOR_LINKS {
        let index = graph.incoming(file, destination_key, meter)??;
        let link = &graph.links[index];
        if link.source.is_some()
            || link.mode != 0
            || graph.key(&link.source_key) != source_key
            || graph.key(&link.source_owner).strip_prefix(b":")
                != Some(b"defaultColorMgtGlobals".as_slice())
        {
            return Err(AppearanceIssue::UnsupportedSettings.into());
        }
        links.push(graph.span(index));
    }
    meter.step(graph.links.len())?;
    for link in &graph.links {
        if link.destination.is_none()
            && graph.key(&link.destination_owner).strip_prefix(b":")
                == Some(b"defaultColorMgtGlobals".as_slice())
            && matches!(
                graph.key(&link.destination_key),
                b"cme"
                    | b"cmEnabled"
                    | b"cfe"
                    | b"cmConfigFileEnabled"
                    | b"cfp"
                    | b"cmConfigFilePath"
                    | b"wsn"
                    | b"workingSpaceName"
            )
        {
            return Err(AppearanceIssue::UnsupportedSettings.into());
        }
    }
    let mut config_enabled = false;
    let mut config = false;
    let mut space = false;
    for record in &graph.facts.records {
        meter.step(1)?;
        let Data::Selection(target) = &record.data else {
            continue;
        };
        if &graph.facts.container.source()[target.clone()] != b":defaultColorMgtGlobals" {
            continue;
        }
        for chunk in children(graph.facts.container, record.group) {
            meter.step(1)?;
            if chunk.tag() == *b"SLCT" {
                continue;
            }
            if !matches!(&chunk.tag(), b"DBLE" | b"STR " | b"FLGS") {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            let field = field(graph.facts.container, chunk)?;
            let key = &graph.facts.container.source()[field.key.clone()];
            if matches!(key, b"cme" | b"cmEnabled") {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            if matches!(key, b"cfe" | b"cmConfigFileEnabled") {
                if chunk.tag() != *b"DBLE" || field.kind != 0x20 {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                if field.value.len() != 8 {
                    return Err(fail(ErrorKind::Malformed, field.value.start).into());
                }
                if config_enabled
                    || f64::from_be_bytes(
                        graph.facts.container.source()[field.value.clone()]
                            .try_into()
                            .unwrap(),
                    ) != 1.0
                {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                config_enabled = true;
                inputs.push(InputEvidence::Saved {
                    property: InputProperty::ColorConfigurationEnabled,
                    span: span(chunk, field.value),
                });
            } else if matches!(
                key,
                b"cfp" | b"cmConfigFilePath" | b"wsn" | b"workingSpaceName"
            ) {
                if chunk.tag() != *b"STR " || field.kind != 0x20 {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                if string_value(graph.facts.container, &field)?.is_empty() {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                let (seen, property) = if matches!(key, b"cfp" | b"cmConfigFilePath") {
                    (&mut config, InputProperty::ColorConfiguration)
                } else {
                    (&mut space, InputProperty::WorkingSpace)
                };
                if *seen {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                *seen = true;
                inputs.push(InputEvidence::Saved {
                    property,
                    span: span(chunk, field.value),
                });
            }
        }
    }
    if !config_enabled || !config || !space {
        return Err(AppearanceIssue::UnsupportedSettings.into());
    }
    inputs.push(InputEvidence::SchemaDefault(
        InputProperty::ColorManagementDefault2025,
    ));
    links.try_into().map_err(|_| resource().into())
}

fn material(
    graph: &Graph<'_>,
    shading_group: usize,
    meter: &mut Meter<'_>,
) -> BindingResult<SourceMaterial> {
    if graph.cyclic {
        return Err(AppearanceIssue::UnsupportedGraph.into());
    }
    let shader_source = graph.incoming(shading_group, b"ss", meter)??;
    if graph.links[shader_source]
        .source
        .is_none_or(|owner| form(graph.facts, owner) != Some(*b"RPHO"))
    {
        return Err(AppearanceIssue::UnsupportedShader.into());
    }
    let shader_link = input(graph, shading_group, b"ss", *b"RPHO", b"oc", 0, meter)?;
    let shader = graph.links[shader_link].source.unwrap();
    let texture_link = input(graph, shader, b"c", *b"RTFT", b"oc", 0, meter)?;
    let file = graph.links[texture_link].source.unwrap();
    let mut placement = None;
    let mut placement_links = meter.vector(PLACEMENT.len())?;
    for (source_key, destination_key) in PLACEMENT {
        let index = input(graph, file, destination_key, *b"RPL2", source_key, 0, meter)?;
        let owner = graph.links[index].source.unwrap();
        if placement.is_some_and(|previous| previous != owner) {
            return Err(AppearanceIssue::UnsupportedGraph.into());
        }
        placement = Some(owner);
        placement_links.push(graph.span(index));
    }
    let placement = placement.ok_or(AppearanceIssue::UnsupportedGraph)?;
    meter.step(graph.facts.records.len())?;
    for record in &graph.facts.records {
        if let Data::Selection(target) = &record.data {
            meter.step((usize::BITS - graph.facts.records.len().leading_zeros()) as usize)?;
            let owner =
                match graph
                    .owners
                    .resolve_name(graph.facts, graph.key(target), target.start)
                {
                    Ok(owner) => owner,
                    Err(error) if error.kind() == ErrorKind::Unsupported => {
                        return Err(AppearanceIssue::UnsupportedGraph.into());
                    }
                    Err(error) => return Err(error.into()),
                };
            if owner.is_some_and(|owner| [shading_group, shader, file, placement].contains(&owner))
            {
                return Err(AppearanceIssue::UnsupportedGraph.into());
            }
        }
    }
    meter.step(graph.links.len())?;
    for link in &graph.links {
        if let Some(owner) = link.destination {
            let key = graph.key(&link.destination_key);
            let valid = if owner == placement {
                false
            } else if owner == shader {
                key == b"c"
            } else if owner == file {
                PLACEMENT.iter().any(|(_, expected)| *expected == key)
                    || COLOR_LINKS.iter().any(|(_, expected)| *expected == key)
            } else if owner == shading_group {
                match key {
                    b"ss" => {
                        link.mode == 0
                            && link.source == Some(shader)
                            && graph.key(&link.source_key) == b"oc"
                    }
                    b"gn" => {
                        link.mode == 1
                            && graph.key(&link.source_key) == b"msg"
                            && link
                                .source
                                .is_some_and(|source| form(graph.facts, source) == Some(*b"GPID"))
                    }
                    b"dsm" => {
                        link.mode == 1
                            && matches!(graph.key(&link.source_key), b"iog.og[0]" | b"iog[0].og[0]")
                            && link
                                .source
                                .is_some_and(|source| form(graph.facts, source) == Some(*b"DMSH"))
                    }
                    _ => false,
                }
            } else {
                true
            };
            if !valid {
                return Err(AppearanceIssue::UnsupportedGraph.into());
            }
        }
        if let Some(owner) = link.source
            && [shading_group, shader, file, placement].contains(&owner)
        {
            let key = graph.key(&link.source_key);
            if message_link(graph.facts, link) {
                continue;
            }
            let valid = if owner == shader {
                key == b"oc"
                    && link.mode == 0
                    && graph.key(&link.destination_key) == b"ss"
                    && link
                        .destination
                        .is_some_and(|owner| form(graph.facts, owner) == Some(*b"SHAD"))
            } else if owner == file {
                key == b"oc"
                    && link.mode == 0
                    && graph.key(&link.destination_key) == b"c"
                    && link
                        .destination
                        .is_some_and(|owner| form(graph.facts, owner) == Some(*b"RPHO"))
            } else if owner == placement {
                link.mode == 0
                    && PLACEMENT.iter().any(|(source, destination)| {
                        *source == key && *destination == graph.key(&link.destination_key)
                    })
                    && link
                        .destination
                        .is_some_and(|owner| form(graph.facts, owner) == Some(*b"RTFT"))
            } else {
                key == b"mwc"
                    && link.mode == 0
                    && matches!(
                        graph.key(&link.destination_key),
                        b"iog.og[0].gco" | b"iog[0].og[0].gco"
                    )
                    && link
                        .destination
                        .is_some_and(|owner| form(graph.facts, owner) == Some(*b"DMSH"))
            };
            if !valid {
                return Err(AppearanceIssue::UnsupportedGraph.into());
            }
        }
    }
    let mut inputs = meter.vector(15)?;
    let mut file_fields = 0usize;
    let mut shader_fields = 0u8;
    let mut shading_group_fields = 0u8;
    for owner in [placement, file, shader, shading_group] {
        for chunk in children(graph.facts.container, graph.facts.records[owner].group) {
            meter.step(1)?;
            if chunk.tag() == *b"CREA" {
                continue;
            }
            check_owner_child(graph.facts, owner, chunk)?;
            if owner == placement {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            if owner == shading_group {
                if chunk.tag() != *b"DBLE" {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                let field = field(graph.facts.container, chunk)?;
                let (property, flag) = match graph.key(&field.key) {
                    b"ihi" | b"isHistoricallyInteresting" => {
                        (InputProperty::ShadingGroupHistoryInterest2026, 1)
                    }
                    b"ro" | b"renderableOnlySet" => {
                        (InputProperty::ShadingGroupRenderableOnlySet2026, 2)
                    }
                    _ => return Err(AppearanceIssue::UnsupportedSettings.into()),
                };
                if field.kind != 0x20 {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                if field.value.len() != 8 {
                    return Err(fail(ErrorKind::Malformed, field.value.start).into());
                }
                let value = f64::from_be_bytes(
                    graph.facts.container.source()[field.value.clone()]
                        .try_into()
                        .unwrap(),
                );
                let valid = if flag == 1 {
                    value.is_finite() && value.fract() == 0.0 && (0.0..=255.0).contains(&value)
                } else {
                    value == 0.0 || value == 1.0
                };
                if !valid || shading_group_fields & flag != 0 {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                shading_group_fields |= flag;
                inputs.push(InputEvidence::Saved {
                    property,
                    span: span(chunk, field.value),
                });
                continue;
            }
            if owner == file && chunk.tag() != *b"STR "
                || owner == shader && !matches!(&chunk.tag(), b"DBLE" | b"FLT3")
            {
                return Err(AppearanceIssue::UnsupportedSettings.into());
            }
            let field = field(graph.facts.container, chunk)?;
            let key = &graph.facts.container.source()[field.key.clone()];
            let property = if owner == file {
                if chunk.tag() != *b"STR " || field.kind != 0x20 {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                let value = string_value(graph.facts.container, &field)?;
                match key {
                    b"ftn" if file_fields & 1 == 0 && !value.is_empty() => {
                        file_fields |= 1;
                        InputProperty::TexturePath
                    }
                    b"cs" if file_fields & 2 == 0 && value == b"sRGB" => {
                        file_fields |= 2;
                        InputProperty::ColorSpace
                    }
                    _ => return Err(AppearanceIssue::UnsupportedSettings.into()),
                }
            } else {
                let (property, flag, tag, kind, length) = match key {
                    b"dc" => (InputProperty::Diffuse, 1, *b"DBLE", 0x24, 8),
                    b"ambc" => (InputProperty::Ambient, 2, *b"FLT3", 0x20, 12),
                    b"sc" => (InputProperty::Specular, 4, *b"FLT3", 0x20, 12),
                    b"rfl" => (InputProperty::Reflectivity, 8, *b"DBLE", 0x20, 8),
                    _ => return Err(AppearanceIssue::UnsupportedSettings.into()),
                };
                if chunk.tag() != tag || field.kind != kind {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                if field.value.len() != length {
                    return Err(fail(ErrorKind::Malformed, field.value.start).into());
                }
                if shader_fields & flag != 0
                    || !finite_words(
                        graph.facts.container.source(),
                        &field.value,
                        if length == 12 { 4 } else { 8 },
                    )
                {
                    return Err(AppearanceIssue::UnsupportedSettings.into());
                }
                shader_fields |= flag;
                property
            };
            inputs.push(InputEvidence::Saved {
                property,
                span: span(chunk, field.value),
            });
        }
    }
    if file_fields != 3 || shader_fields != 15 {
        return Err(AppearanceIssue::UnsupportedSettings.into());
    }
    for (property, flag) in [
        (InputProperty::ShadingGroupHistoryInterest2026, 1),
        (InputProperty::ShadingGroupRenderableOnlySet2026, 2),
    ] {
        if shading_group_fields & flag == 0 {
            inputs.push(InputEvidence::SchemaDefault(property));
        }
    }
    for property in [
        InputProperty::PlacementCompoundDefaults,
        InputProperty::TextureCompoundDefaults,
        InputProperty::PhongTransparencyDefault,
    ] {
        inputs.push(InputEvidence::SchemaDefault(property));
    }
    let color_links = color_inputs(graph, file, &mut inputs, meter)?;
    let owners = [shading_group, shader, file, placement];
    meter.step(graph.links.len())?;
    let message_count = graph
        .links
        .iter()
        .filter(|link| {
            link.source.is_some_and(|owner| owners.contains(&owner))
                && message_link(graph.facts, link)
        })
        .count();
    let mut message_links = meter.vector(message_count)?;
    for link in &graph.links {
        if link.source.is_some_and(|owner| owners.contains(&owner))
            && message_link(graph.facts, link)
        {
            let chunk = graph.facts.container.chunk(link.chunk).unwrap();
            message_links.push(SourceMessage {
                span: span(chunk, chunk.payload_range()),
                mode: link.mode,
                source: link.source_owner.start..link.source_key.end,
                destination: link.destination_owner.start..link.destination_key.end,
            });
        }
    }
    Ok(SourceMaterial {
        owners: owners.map(|owner| graph.facts.records[owner].group),
        shader_link: graph.span(shader_link),
        texture_link: graph.span(texture_link),
        placement_links: placement_links.try_into().map_err(|_| resource())?,
        color_links,
        inputs,
        message_links,
    })
}

/// Each mesh record's face count, built once per `bind` so `preflight` and `shape_binding` look
/// it up instead of scanning the bindings and the scene's shapes for every mesh (#380).
struct FaceCounts(HashMap<usize, usize>);

impl FaceCounts {
    fn new(
        bindings: &[Binding],
        scene: &NativeScene,
        meter: &mut Meter<'_>,
    ) -> Result<Self, BinaryError> {
        let shapes = scene.shapes();
        meter.step(
            bindings
                .len()
                .checked_add(shapes.len())
                .ok_or_else(resource)?,
        )?;
        meter.charge_map::<NativeId, usize>(shapes.len())?;
        meter.charge_map::<usize, usize>(bindings.len())?;
        let mut by_id = HashMap::new();
        by_id.try_reserve(shapes.len()).map_err(|_| resource())?;
        for shape in shapes {
            by_id
                .entry(shape.id())
                .or_insert(shape.mesh().faces().len());
        }
        let mut map = HashMap::new();
        map.try_reserve(bindings.len()).map_err(|_| resource())?;
        for binding in bindings {
            if let Some(faces) = by_id.get(&binding.id) {
                map.entry(binding.record).or_insert(*faces);
            }
        }
        Ok(Self(map))
    }

    /// The face count of the shape bound to `record`, or the resource error the old scan returned
    /// for a record with no binding. Every mesh binding has its shape by the time `bind` runs
    /// (`binding::construct` pushes it first), so a binding without one cannot reach here.
    fn by_record(&self, record: usize) -> Result<usize, BinaryError> {
        self.0.get(&record).copied().ok_or_else(resource)
    }
}

/// Each DAG child's parent, built once per `bind` so the per-shape instance and ancestor walks
/// are lookups rather than a scan of every edge for every shape (#380). A child with more than
/// one incoming edge is recorded as ambiguous, which is what the scans refused.
struct Parents {
    map: HashMap<NativeId, Option<Option<NativeId>>>,
    /// The ancestor walk's iteration bound: one more step than there are transforms.
    depth_bound: usize,
}

impl Parents {
    fn new(scene: &NativeScene, meter: &mut Meter<'_>) -> Result<Self, BinaryError> {
        let edges = scene.dag_edges();
        meter.step(edges.len())?;
        meter.charge_map::<NativeId, Option<Option<NativeId>>>(edges.len())?;
        let mut map = HashMap::new();
        map.try_reserve(edges.len()).map_err(|_| resource())?;
        for edge in edges {
            map.entry(edge.child())
                .and_modify(|parent: &mut Option<Option<NativeId>>| *parent = None)
                .or_insert(Some(edge.parent()));
        }
        Ok(Self {
            map,
            depth_bound: scene.nodes().len(),
        })
    }

    /// The one parent of `shape`, after checking that it and every ancestor up to the root has
    /// exactly one incoming edge, so the shape has a single instance path. A shape with no
    /// parent, any ancestor reached more than once, or a walk longer than the transform count
    /// is `AmbiguousInstance`.
    fn single_path_parent(
        &self,
        shape: NativeId,
        meter: &mut Meter<'_>,
    ) -> BindingResult<NativeId> {
        meter.step(1)?;
        let parent = self
            .of(shape)?
            .flatten()
            .ok_or(AppearanceIssue::AmbiguousInstance)?;
        let mut ancestor = Some(parent);
        for _ in 0..=self.depth_bound {
            let Some(id) = ancestor else { break };
            meter.step(1)?;
            ancestor = self.of(id)?.flatten();
        }
        if ancestor.is_some() {
            return Err(AppearanceIssue::AmbiguousInstance.into());
        }
        Ok(parent)
    }

    /// The parent of `child` (`Some(None)` for a root), `None` when it has no edge, or
    /// `AmbiguousInstance` when it has several.
    fn of(&self, child: NativeId) -> BindingResult<Option<Option<NativeId>>> {
        match self.map.get(&child) {
            None => Ok(None),
            Some(Some(parent)) => Ok(Some(*parent)),
            Some(None) => Err(AppearanceIssue::AmbiguousInstance.into()),
        }
    }
}

fn shape_binding(
    container: &Container,
    facts: &WireFacts<'_>,
    graph: &Graph<'_>,
    binding: &Binding,
    parents: &Parents,
    faces_of: &FaceCounts,
    meter: &mut Meter<'_>,
) -> BindingResult<(SourceSurfaceBinding, SourceMaterial)> {
    let record_index = binding.mesh.ok_or(AppearanceIssue::UnsupportedGraph)?;
    let record = &facts.records[record_index];
    let Data::Mesh(mesh) = &record.data else {
        return Err(AppearanceIssue::UnsupportedGraph.into());
    };
    let mesh_chunk = container.chunk(record.chunk).unwrap();
    let parent = parents.single_path_parent(binding.id, meter)?;
    meter.step(1)?;
    let faces = faces_of.by_record(binding.record)?;
    let mut component_record = None;
    let mut face_range = None;
    for chunk in children(container, binding.group) {
        meter.step(1)?;
        if chunk.tag() != *b"CMP#" {
            continue;
        }
        check_owner_child(facts, binding.record, chunk)?;
        let value = field(container, chunk)?;
        if &container.source()[value.key.clone()] != b"iog[0].og[0].gcl" {
            continue;
        }
        let range = component(container, chunk, &value, faces)?;
        if component_record.replace(span(chunk, value.value)).is_some() {
            return Err(AppearanceIssue::OverlappingMembership.into());
        }
        face_range = Some(range);
    }
    let component = component_record.ok_or(AppearanceIssue::MissingMembership)?;
    let mut membership = None;
    meter.step(graph.links.len())?;
    for (index, link) in graph.links.iter().enumerate() {
        if link.source != Some(binding.record) {
            continue;
        }
        let key = graph.key(&link.source_key);
        let supported = matches!(key, b"iog.og[0]" | b"iog[0].og[0]");
        if !supported {
            return Err(AppearanceIssue::OverlappingMembership.into());
        }
        if membership.replace(index).is_some() {
            return Err(AppearanceIssue::OverlappingMembership.into());
        }
        if link.mode != 1
            || graph.key(&link.destination_key) != b"dsm"
            || link
                .destination
                .is_none_or(|owner| form(facts, owner) != Some(*b"SHAD"))
        {
            return Err(AppearanceIssue::UnsupportedGraph.into());
        }
    }
    let membership = membership.ok_or(AppearanceIssue::MissingMembership)?;
    let shading_group = graph.links[membership].destination.unwrap();
    let group_id_link = input(
        graph,
        binding.record,
        b"iog.og[0].gid",
        *b"GPID",
        b"id",
        0,
        meter,
    )?;
    let group_id = graph.links[group_id_link].source.unwrap();
    let group_link = input(graph, shading_group, b"gn", *b"GPID", b"msg", 1, meter)?;
    if graph.links[group_link].source != Some(group_id) {
        return Err(AppearanceIssue::UnsupportedGraph.into());
    }
    let group_color_link = input(
        graph,
        binding.record,
        b"iog.og[0].gco",
        *b"SHAD",
        b"mwc",
        0,
        meter,
    )?;
    if graph.links[group_color_link].source != Some(shading_group) {
        return Err(AppearanceIssue::UnsupportedGraph.into());
    }
    let (explicit_uv, explicit_uv_name, active_uv_name, packed_uv) =
        uv_binding(container, facts, binding.record, mesh, mesh_chunk, meter)?;
    let material = material(graph, shading_group, meter)?;
    Ok((
        SourceSurfaceBinding {
            faces: face_range.ok_or(AppearanceIssue::MissingMembership)?,
            instance_parent: parent,
            component,
            group_id: facts.records[group_id].group,
            group_id_link: graph.span(group_id_link),
            membership_link: graph.span(membership),
            group_link: graph.span(group_link),
            group_color_link: graph.span(group_color_link),
            uv_name: span(mesh_chunk, mesh.uv_name.clone()),
            explicit_uv_name,
            active_uv_name,
            explicit_uv,
            packed_uv,
            corner_ids: span(mesh_chunk, mesh.uv_ids.range.clone()),
            material: 0,
        },
        material,
    ))
}

fn checked_material_shape(
    graph: &Graph<'_>,
    shapes: &[ShapeAppearance],
    materials: &[SourceMaterial],
    owner: Option<usize>,
    shading_group: usize,
    meter: &mut Meter<'_>,
) -> Result<bool, BinaryError> {
    let Some(owner) = owner else { return Ok(false) };
    meter.step(shapes.len())?;
    Ok(shapes.iter().any(|shape| {
        shape.source_group == graph.facts.records[owner].group
            && shape.binding.as_ref().is_ok_and(|binding| {
                materials[binding.material].owners[0] == graph.facts.records[shading_group].group
            })
    }))
}

fn complete_diffuse_users(
    graph: &Graph<'_>,
    shader: usize,
    shapes: &[ShapeAppearance],
    materials: &[SourceMaterial],
    meter: &mut Meter<'_>,
) -> Result<bool, BinaryError> {
    let mut groups = 0usize;
    meter.step(graph.links.len())?;
    for output in &graph.links {
        if output.source != Some(shader) || message_link(graph.facts, output) {
            continue;
        }
        let Some(group) = output.destination else {
            return Ok(false);
        };
        if output.mode != 0
            || graph.key(&output.source_key) != b"oc"
            || graph.key(&output.destination_key) != b"ss"
            || form(graph.facts, group) != Some(*b"SHAD")
        {
            return Ok(false);
        }
        groups += 1;
        let mut members = 0usize;
        meter.step(graph.links.len())?;
        for link in &graph.links {
            if link.destination == Some(group) && graph.key(&link.destination_key) == b"dsm" {
                if !checked_material_shape(graph, shapes, materials, link.source, group, meter)? {
                    return Ok(false);
                }
                members += 1;
            }
            if link.source == Some(group)
                && !message_link(graph.facts, link)
                && !checked_material_shape(
                    graph,
                    shapes,
                    materials,
                    link.destination,
                    group,
                    meter,
                )?
            {
                return Ok(false);
            }
        }
        if members == 0 {
            return Ok(false);
        }
    }
    Ok(groups != 0)
}

fn bind_diffuse(
    graph: &Graph<'_>,
    shapes: &[ShapeAppearance],
    materials: &[SourceMaterial],
    meter: &mut Meter<'_>,
) -> Result<(Vec<DiffuseBinding>, Vec<MaterialDiffuse>), BinaryError> {
    let mut bindings: Vec<DiffuseBinding> = meter.vector(materials.len())?;
    let mut values = meter.vector(materials.len())?;
    for material in materials {
        let shader = material.owners[1];
        meter.step(bindings.len())?;
        if bindings.iter().any(|binding| binding.shader == shader) {
            continue;
        }
        let Some(span) = material.inputs.iter().find_map(|input| match input {
            InputEvidence::Saved {
                property: InputProperty::Diffuse,
                span,
            } => Some(span),
            _ => None,
        }) else {
            continue;
        };
        meter.step(graph.facts.records.len())?;
        let shader_record = graph
            .facts
            .records
            .iter()
            .position(|record| record.group == shader && matches!(record.data, Data::Create(_)))
            .ok_or_else(|| {
                fail(
                    ErrorKind::Malformed,
                    graph.facts.container.chunks()[shader].header_range().start,
                )
            })?;
        if !complete_diffuse_users(graph, shader_record, shapes, materials, meter)? {
            continue;
        }
        meter.step(shapes.len())?;
        let uses_shader = |shape: &ShapeAppearance| {
            shape.binding.as_ref().is_ok_and(|binding| {
            let owner = &materials[binding.material];
            owner.owners[1] == shader && owner.inputs.iter().any(|input| {
                matches!(input, InputEvidence::Saved { property: InputProperty::Diffuse, span: other } if other == span)
            })
        })
        };
        let mut users = meter.vector(shapes.iter().filter(|shape| uses_shader(shape)).count())?;
        meter.step(shapes.len())?;
        for shape in shapes.iter().filter(|shape| uses_shader(shape)) {
            users.push(shape.shape);
        }
        if users.is_empty() {
            continue;
        }
        let word: [u8; 8] = graph.facts.container.source()[span.value.clone()]
            .try_into()
            .map_err(|_| fail(ErrorKind::Malformed, span.value.start))?;
        values.push(MaterialDiffuse {
            binding: bindings.len(),
            value: f64::from_be_bytes(word),
            replaced: false,
        });
        bindings.push(DiffuseBinding {
            shader,
            span: span.clone(),
            shapes: users,
        });
    }
    Ok((bindings, values))
}

pub(super) fn bind(
    container: &Container,
    facts: &WireFacts<'_>,
    bindings: &[Binding],
    owners: &Owners,
    scene: &NativeScene,
    budget: &mut Budget,
    retained_appearance_bytes: usize,
) -> Result<(Arc<SourceAppearance>, Vec<MaterialDiffuse>), BinaryError> {
    let mut meter = Meter {
        budget,
        bytes: 0,
        steps: 0,
    };
    meter.charge(retained_appearance_bytes)?;
    meter.charge(owners.metadata_bytes()?)?;
    let promoted = size_of::<Range<usize>>()
        .checked_add(2 * size_of::<crate::maya_binary::semantic::Words>())
        .ok_or_else(resource)?;
    meter.charge(
        facts
            .records
            .capacity()
            .checked_mul(promoted)
            .ok_or_else(resource)?,
    )?;
    let graph = Graph::new(facts, owners, &mut meter)?;
    let faces_of = FaceCounts::new(bindings, scene, &mut meter)?;
    preflight(&graph, &faces_of, &mut meter)?;
    let mut shapes = meter.vector(scene.shapes().len())?;
    let mut materials = meter.vector::<SourceMaterial>(scene.shapes().len())?;
    let parents = Parents::new(scene, &mut meter)?;
    for binding in bindings.iter().filter(|binding| binding.mesh.is_some()) {
        let outcome = match shape_binding(
            container, facts, &graph, binding, &parents, &faces_of, &mut meter,
        ) {
            Ok((mut surface, material)) => {
                meter.step(materials.len())?;
                let material_index =
                    if let Some(index) = materials.iter().position(|entry| entry == &material) {
                        index
                    } else {
                        let index = materials.len();
                        materials.push(material);
                        index
                    };
                surface.material = material_index;
                Ok(surface)
            }
            Err(BindingFailure::Unavailable(issue)) => Err(issue),
            Err(BindingFailure::Fatal(error)) => return Err(error),
        };
        shapes.push(ShapeAppearance {
            shape: binding.id,
            source_group: binding.group,
            binding: outcome,
        });
    }
    let (diffuse, values) = bind_diffuse(&graph, &shapes, &materials, &mut meter)?;
    let arc_bytes = size_of::<SourceAppearance>()
        .checked_add(2 * size_of::<usize>())
        .ok_or_else(resource)?;
    meter.charge(arc_bytes)?;
    meter.budget.charge(arc_bytes)?;
    Ok((
        Arc::new(SourceAppearance {
            shapes,
            materials,
            diffuse,
        }),
        values,
    ))
}
