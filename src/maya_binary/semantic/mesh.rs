use super::{BinaryError, Cursor, ErrorKind, Span, Words, error};

#[derive(Debug)]
pub(crate) struct Mesh {
    pub key: Span,
    #[cfg(test)]
    pub kind_offset: usize,
    pub positions: Words,
    pub endpoints: Words,
    pub corners: Words,
    #[cfg(test)]
    pub normals: Words,
    #[cfg(test)]
    pub uv_header: Span,
    pub uv_name: Span,
    pub uv_points: Words,
    pub uv_ids: Words,
    #[cfg(test)]
    pub trailer: Span,
}

pub(crate) fn corner(word: u32, offset: usize) -> Result<(usize, bool, bool), BinaryError> {
    let (reverse, terminal) = match word >> 24 {
        0x00 => (false, false),
        0x80 => (true, false),
        0x60 => (false, true),
        0xe0 => (true, true),
        _ => return Err(error(offset, ErrorKind::Unsupported)),
    };
    Ok(((word & 0x00ff_ffff) as usize, reverse, terminal))
}

pub(crate) fn decode_mesh(cursor: &mut Cursor<'_>) -> Result<Mesh, BinaryError> {
    let key = cursor.string()?;
    let kind_offset = cursor.position;
    if cursor.byte()? != 0x20 {
        return Err(error(kind_offset, ErrorKind::Unsupported));
    }
    let positions = cursor.words(3)?;
    let endpoints = cursor.words(2)?;
    let corners = cursor.words(1)?;
    let _normals = cursor.words(3)?;
    let uv_start = cursor.position;
    if cursor.u32()? != 0 || cursor.u32()? != 1 || cursor.u32()? != 0 {
        return Err(error(uv_start, ErrorKind::Unsupported));
    }
    let _uv_header = uv_start..cursor.position;
    let _uv_name = cursor.string()?;
    let uv_points = cursor.words(2)?;
    let uv_ids = cursor.words(1)?;
    // An empty set (no points and no ids) is the one shape that need not match the corner
    // count; a set with points but no ids, or ids but no points, still must.
    let empty_uv = uv_points.count == 0 && uv_ids.count == 0;
    if uv_ids.count != corners.count && !empty_uv {
        return Err(error(uv_ids.range.start, ErrorKind::Layout));
    }
    let trailer = cursor.take(20)?;
    if cursor.source[trailer.clone()].iter().any(|byte| *byte != 0) {
        return Err(error(trailer.start, ErrorKind::Unsupported));
    }
    let mut endpoint_words = endpoints.values(cursor.source);
    while let Some(first) = endpoint_words.next() {
        let second = endpoint_words.next().unwrap();
        if second & 0x8000_0000 != 0 {
            return Err(error(endpoints.range.start, ErrorKind::Unsupported));
        }
        if (first & 0x7fff_ffff) as usize >= positions.count / 3
            || second as usize >= positions.count / 3
        {
            return Err(error(endpoints.range.start, ErrorKind::Reference));
        }
    }
    let mut in_face = false;
    for (index, word) in corners.values(cursor.source).enumerate() {
        let offset = corners.range.start + index * 4;
        let (edge, _, terminal) = corner(word, offset)?;
        if edge >= endpoints.count / 2 {
            return Err(error(offset, ErrorKind::Reference));
        }
        in_face = !terminal;
    }
    if in_face {
        return Err(error(corners.range.end, ErrorKind::Layout));
    }
    if uv_ids
        .values(cursor.source)
        .any(|index| index as usize >= uv_points.count / 2)
    {
        return Err(error(uv_ids.range.start, ErrorKind::Reference));
    }
    Ok(Mesh {
        key,
        #[cfg(test)]
        kind_offset,
        positions,
        endpoints,
        corners,
        #[cfg(test)]
        normals: _normals,
        #[cfg(test)]
        uv_header: _uv_header,
        uv_name: _uv_name,
        uv_points,
        uv_ids,
        #[cfg(test)]
        trailer,
    })
}
