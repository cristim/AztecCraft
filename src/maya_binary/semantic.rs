pub(crate) mod mesh;
pub(crate) mod records;
#[cfg(test)]
mod tests;

use super::container::{Chunk, Container};
use std::{fmt, mem::size_of, ops::Range};

const MAX_FACT_METADATA_BYTES: usize = 32 * 1024 * 1024;
type Span = Range<usize>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ErrorKind {
    Truncated,
    Unsupported,
    Layout,
    Reference,
    Owner,
    Resource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BinaryError {
    pub(crate) offset: Option<usize>,
    pub(crate) kind: ErrorKind,
}

impl fmt::Display for BinaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "binary wire {:?} at byte {:?}",
            self.kind, self.offset
        )
    }
}

impl std::error::Error for BinaryError {}

pub(crate) fn global_error(kind: ErrorKind) -> BinaryError {
    BinaryError { offset: None, kind }
}

pub(crate) fn error(offset: usize, kind: ErrorKind) -> BinaryError {
    BinaryError {
        offset: Some(offset),
        kind,
    }
}

pub(crate) struct Cursor<'a> {
    source: &'a [u8],
    position: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a [u8], range: Span) -> Result<Self, BinaryError> {
        if range.start > range.end || range.end > source.len() {
            return Err(error(range.start, ErrorKind::Truncated));
        }
        Ok(Self {
            source,
            position: range.start,
            end: range.end,
        })
    }

    fn take(&mut self, count: usize) -> Result<Span, BinaryError> {
        let end = self
            .position
            .checked_add(count)
            .filter(|end| *end <= self.end)
            .ok_or_else(|| error(self.position, ErrorKind::Truncated))?;
        let range = self.position..end;
        self.position = end;
        Ok(range)
    }

    fn byte(&mut self) -> Result<u8, BinaryError> {
        let range = self.take(1)?;
        Ok(self.source[range.start])
    }

    fn u32(&mut self) -> Result<u32, BinaryError> {
        let range = self.take(4)?;
        let (_, value) =
            nom::number::complete::be_u32::<_, nom::error::Error<_>>(&self.source[range])
                .map_err(|_| error(self.position - 4, ErrorKind::Truncated))?;
        Ok(value)
    }

    fn string_before(&mut self, end: usize) -> Result<Span, BinaryError> {
        if end > self.end || self.position >= end {
            return Err(error(self.position, ErrorKind::Truncated));
        }
        let count = self.source[self.position..end]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| error(self.position, ErrorKind::Truncated))?;
        let string = self.take(count)?;
        self.take(1)?;
        Ok(string)
    }

    fn string(&mut self) -> Result<Span, BinaryError> {
        self.string_before(self.end)
    }

    fn words(&mut self, divisor: usize) -> Result<Words, BinaryError> {
        let count_offset = self.position;
        let count =
            usize::try_from(self.u32()?).map_err(|_| error(count_offset, ErrorKind::Resource))?;
        if !count.is_multiple_of(divisor) {
            return Err(error(count_offset, ErrorKind::Layout));
        }
        let bytes = count
            .checked_mul(4)
            .ok_or_else(|| error(count_offset, ErrorKind::Resource))?;
        Ok(Words {
            range: self.take(bytes)?,
            count,
        })
    }

    fn finish(self) -> Result<(), BinaryError> {
        if self.position != self.end {
            return Err(error(self.position, ErrorKind::Layout));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Words {
    pub(crate) range: Span,
    pub(crate) count: usize,
}

impl Words {
    pub(crate) fn values<'a>(&self, source: &'a [u8]) -> impl Iterator<Item = u32> + 'a {
        source[self.range.clone()]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_be_bytes(*word))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Owner {
    Created(usize),
    Selected(usize),
}

#[derive(Debug)]
pub(crate) enum Data {
    Create(records::Create),
    Double(records::Double),
    Selection(Span),
    Connection(records::Connection),
    Relation(records::Relation),
    Mesh(mesh::Mesh),
}

#[derive(Debug)]
pub(crate) struct Record {
    pub(crate) chunk: usize,
    pub(crate) group: usize,
    pub(crate) owner: Option<Owner>,
    pub(crate) data: Data,
}

pub(crate) struct WireFacts<'a> {
    pub(crate) container: &'a Container,
    pub(crate) records: Vec<Record>,
    pub(crate) owners: Vec<Option<Owner>>,
}

impl WireFacts<'_> {
    pub(crate) fn metadata_bytes(&self) -> Result<usize, BinaryError> {
        metadata_bytes(self.records.capacity(), self.owners.capacity())
    }
}

impl fmt::Debug for WireFacts<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WireFacts")
            .field("source_len", &self.container.source().len())
            .field("record_count", &self.records.len())
            .field("owner_slots", &self.owners.len())
            .finish()
    }
}

pub(crate) fn metadata_bytes(records: usize, owners: usize) -> Result<usize, BinaryError> {
    let bytes = records
        .checked_mul(size_of::<Record>())
        .and_then(|bytes| {
            owners
                .checked_mul(size_of::<Option<Owner>>())
                .and_then(|owner_bytes| bytes.checked_add(owner_bytes))
        })
        .filter(|bytes| *bytes <= MAX_FACT_METADATA_BYTES)
        .ok_or_else(|| global_error(ErrorKind::Resource))?;
    Ok(bytes)
}

pub(crate) fn recognized(tag: [u8; 4]) -> bool {
    matches!(
        &tag,
        b"CREA" | b"DBL3" | b"DBLE" | b"SLCT" | b"CWFL" | b"RELA" | b"MESH"
    )
}

pub(crate) fn require_connection_group(
    container: &Container,
    chunk: &Chunk,
) -> Result<(), BinaryError> {
    let group = chunk
        .parent()
        .and_then(|index| container.chunk(index))
        .filter(|group| group.form() == Some(*b"CONN"))
        .ok_or_else(|| error(chunk.header_range().start, ErrorKind::Owner))?;
    let parent = group
        .parent()
        .and_then(|index| container.chunk(index))
        .filter(|group| group.tag() == *b"LIS8" && group.form() == Some(*b"CONS"));
    if parent.is_none() {
        return Err(error(group.header_range().start, ErrorKind::Owner));
    }
    Ok(())
}

pub(crate) fn decode_records(container: &Container) -> Result<WireFacts<'_>, BinaryError> {
    let record_count = container
        .chunks()
        .iter()
        .filter(|chunk| chunk.form().is_none() && recognized(chunk.tag()))
        .count();
    metadata_bytes(record_count, container.chunks().len())?;
    let mut facts = WireFacts {
        container,
        records: Vec::new(),
        owners: Vec::new(),
    };
    facts
        .records
        .try_reserve_exact(record_count)
        .map_err(|_| global_error(ErrorKind::Resource))?;
    facts
        .owners
        .try_reserve_exact(container.chunks().len())
        .map_err(|_| global_error(ErrorKind::Resource))?;
    metadata_bytes(facts.records.capacity(), facts.owners.capacity())?;
    facts.owners.resize(container.chunks().len(), None);
    let source = container.source();
    let mut cons_count = 0;
    let mut conn_count = 0;
    for (index, chunk) in container.chunks().iter().enumerate() {
        if let Some(form) = chunk.form() {
            match &form {
                b"CONS" => cons_count += 1,
                b"CONN" => conn_count += 1,
                _ => {}
            }
            if cons_count > 1 || conn_count > 1 {
                return Err(error(chunk.header_range().start, ErrorKind::Owner));
            }
            continue;
        }
        if !recognized(chunk.tag()) {
            continue;
        }
        let group = chunk
            .parent()
            .ok_or_else(|| error(chunk.header_range().start, ErrorKind::Owner))?;
        let parent = container.chunk(group).unwrap();
        let offset = chunk.payload_range().start;
        let mut cursor = Cursor::new(source, chunk.payload_range())?;
        let owner = facts.owners[group];
        let data = match &chunk.tag() {
            b"CREA" => {
                if owner.is_some() || parent.form() == Some(*b"SLCT") {
                    return Err(error(offset, ErrorKind::Owner));
                }
                let create = records::decode_create(&mut cursor)?;
                facts.owners[group] = Some(Owner::Created(facts.records.len()));
                Data::Create(create)
            }
            b"SLCT" => {
                if owner.is_some()
                    || parent.form() != Some(*b"SLCT")
                    || chunk.header_range().start != parent.payload_range().start + 4
                {
                    return Err(error(offset, ErrorKind::Owner));
                }
                let target = cursor.take(cursor.end - cursor.position)?;
                if source[target.clone()].first() != Some(&b':')
                    || source[target.clone()].contains(&0)
                {
                    return Err(error(offset, ErrorKind::Unsupported));
                }
                facts.owners[group] = Some(Owner::Selected(facts.records.len()));
                Data::Selection(target)
            }
            b"DBL3" | b"DBLE" => {
                if owner.is_none() {
                    return Err(error(offset, ErrorKind::Owner));
                }
                Data::Double(records::decode_double(
                    &mut cursor,
                    chunk.tag() == *b"DBL3",
                )?)
            }
            b"MESH" => {
                if !matches!(owner, Some(Owner::Created(_))) || parent.form() != Some(*b"DMSH") {
                    return Err(error(offset, ErrorKind::Owner));
                }
                Data::Mesh(mesh::decode_mesh(&mut cursor)?)
            }
            b"CWFL" => {
                require_connection_group(container, chunk)?;
                Data::Connection(records::decode_connection(&mut cursor)?)
            }
            b"RELA" => {
                require_connection_group(container, chunk)?;
                Data::Relation(records::decode_relation(&mut cursor)?)
            }
            _ => unreachable!(),
        };
        cursor.finish()?;
        facts.records.push(Record {
            chunk: index,
            group,
            owner,
            data,
        });
    }
    for (index, chunk) in container.chunks().iter().enumerate() {
        if chunk.form() == Some(*b"SLCT")
            && !matches!(facts.owners[index], Some(Owner::Selected(_)))
        {
            return Err(error(chunk.header_range().start, ErrorKind::Owner));
        }
    }
    Ok(facts)
}
