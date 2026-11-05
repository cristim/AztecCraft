use super::{BinaryError, Cursor, ErrorKind, Span, error};

#[derive(Debug)]
pub(crate) struct Create {
    #[cfg(test)]
    pub flags: u8,
    pub name: Span,
    pub parent: Option<Span>,
    #[cfg(test)]
    pub identity: Span,
}

pub(crate) fn decode_create(cursor: &mut Cursor<'_>) -> Result<Create, BinaryError> {
    let offset = cursor.position;
    let flags = cursor.byte()?;
    let has_parent = match flags {
        0x04 | 0x05 | 0x0c => true,
        0x80 | 0x81 | 0x8c | 0x8d => false,
        _ => return Err(error(offset, ErrorKind::Unsupported)),
    };
    let identity_start = cursor
        .end
        .checked_sub(16)
        .filter(|end| *end >= cursor.position)
        .ok_or_else(|| error(offset, ErrorKind::Truncated))?;
    let name = cursor.string_before(identity_start)?;
    let parent = has_parent
        .then(|| cursor.string_before(identity_start))
        .transpose()?;
    if cursor.position != identity_start {
        return Err(error(cursor.position, ErrorKind::Layout));
    }
    let _identity = cursor.take(16)?;
    Ok(Create {
        #[cfg(test)]
        flags,
        name,
        parent,
        #[cfg(test)]
        identity: _identity,
    })
}

#[derive(Debug)]
pub(crate) struct Double {
    pub key: Span,
    pub kind: u8,
    pub numeric: Span,
}

pub(crate) fn decode_double(cursor: &mut Cursor<'_>, triple: bool) -> Result<Double, BinaryError> {
    let key = cursor.string()?;
    let offset = cursor.position;
    let kind = cursor.byte()?;
    if (triple && kind != 0x20) || (!triple && !matches!(kind, 0x20 | 0x24 | 0x30)) {
        return Err(error(offset, ErrorKind::Unsupported));
    }
    let numeric = cursor.take(if triple { 24 } else { 8 })?;
    Ok(Double { key, kind, numeric })
}

#[derive(Debug)]
pub(crate) struct Connection {
    pub mode: u8,
    pub source: Span,
    pub destination: Span,
}

pub(crate) fn decode_connection(cursor: &mut Cursor<'_>) -> Result<Connection, BinaryError> {
    let offset = cursor.position;
    let mode = cursor.byte()?;
    if !matches!(mode, 0 | 1) {
        return Err(error(offset, ErrorKind::Unsupported));
    }
    let source = cursor.string()?;
    let destination = cursor.string()?;
    Ok(Connection {
        mode,
        source,
        destination,
    })
}

#[derive(Debug)]
pub(crate) struct Relation {
    #[cfg(test)]
    pub kind: Span,
    pub head: Span,
    #[cfg(test)]
    pub count_word: Span,
    pub tails: [Span; 2],
}

pub(crate) fn decode_relation(cursor: &mut Cursor<'_>) -> Result<Relation, BinaryError> {
    let kind = cursor.string()?;
    if !matches!(&cursor.source[kind.clone()], b"link" | b"shadowLink") {
        return Err(error(kind.start, ErrorKind::Unsupported));
    }
    let head = cursor.string()?;
    let offset = cursor.position;
    if cursor.u32()? != 2 {
        return Err(error(offset, ErrorKind::Unsupported));
    }
    let _count_word = offset..cursor.position;
    let tails = [cursor.string()?, cursor.string()?];
    Ok(Relation {
        #[cfg(test)]
        kind,
        head,
        #[cfg(test)]
        count_word: _count_word,
        tails,
    })
}
