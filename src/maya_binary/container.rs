use nom::number::complete::{be_u32, be_u64};
use std::{fmt, ops::Range, sync::Arc};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CHUNKS: usize = 100_000;
pub const MAX_DEPTH: usize = 64;
const HEADER_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    ByteLimit,
    ChunkLimit,
    DepthLimit,
    Truncated,
    Overflow,
    InvalidRoot,
    Unsupported,
    TrailingBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerError {
    offset: usize,
    kind: ErrorKind,
}

impl ContainerError {
    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "binary container {:?} at byte {}",
            self.kind, self.offset
        )
    }
}

impl std::error::Error for ContainerError {}

#[derive(Debug)]
pub struct Chunk {
    tag: [u8; 4],
    form: Option<[u8; 4]>,
    auxiliary: u32,
    parent: Option<usize>,
    header: Range<usize>,
    payload: Range<usize>,
    padding: Range<usize>,
}

impl Chunk {
    pub fn tag(&self) -> [u8; 4] {
        self.tag
    }

    pub fn form(&self) -> Option<[u8; 4]> {
        self.form
    }

    pub fn auxiliary(&self) -> u32 {
        self.auxiliary
    }

    pub fn parent(&self) -> Option<usize> {
        self.parent
    }

    pub fn header_range(&self) -> Range<usize> {
        self.header.clone()
    }

    pub fn payload_range(&self) -> Range<usize> {
        self.payload.clone()
    }

    pub fn padding_range(&self) -> Range<usize> {
        self.padding.clone()
    }
}

pub struct Container {
    source: Arc<[u8]>,
    chunks: Vec<Chunk>,
}

impl fmt::Debug for Container {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Container")
            .field("source_len", &self.source.len())
            .field("chunk_count", &self.chunks.len())
            .finish()
    }
}

impl Container {
    pub(crate) fn metadata_bytes(&self) -> Option<usize> {
        self.chunks
            .capacity()
            .checked_mul(std::mem::size_of::<Chunk>())
    }

    pub fn source(&self) -> &Arc<[u8]> {
        &self.source
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    pub fn chunk(&self, index: usize) -> Option<&Chunk> {
        self.chunks.get(index)
    }

    pub fn payload(&self, index: usize) -> Option<&[u8]> {
        self.chunk(index)
            .map(|chunk| &self.source[chunk.payload.clone()])
    }
}

#[derive(Clone, Copy)]
struct Frame {
    parent: usize,
    next: usize,
    end: usize,
    alignment: usize,
}

pub fn inspect(source: Arc<[u8]>) -> Result<Container, ContainerError> {
    if source.len() > MAX_BYTES {
        return Err(error(0, ErrorKind::ByteLimit));
    }
    let (root, root_alignment) = read_chunk(&source, 0, source.len(), 1, None)?;
    if root.tag != *b"FOR8" || root.form != Some(*b"Maya") {
        return Err(error(0, ErrorKind::InvalidRoot));
    }
    if root.padding.end != source.len() {
        return Err(error(root.padding.end, ErrorKind::TrailingBytes));
    }
    let mut stack = vec![Frame {
        parent: 0,
        next: root.payload.start + 4,
        end: root.payload.end,
        alignment: root_alignment.ok_or_else(|| error(0, ErrorKind::InvalidRoot))?,
    }];
    let mut chunks = vec![root];
    while let Some(frame) = stack.last().copied() {
        if frame.next == frame.end {
            stack.pop();
            continue;
        }
        if chunks.len() == MAX_CHUNKS {
            return Err(error(frame.next, ErrorKind::ChunkLimit));
        }
        let (chunk, alignment) = read_chunk(
            &source,
            frame.next,
            frame.end,
            frame.alignment,
            Some(frame.parent),
        )?;
        stack.last_mut().unwrap().next = chunk.padding.end;
        if let Some(alignment) = alignment {
            if stack.len() == MAX_DEPTH {
                return Err(error(frame.next, ErrorKind::DepthLimit));
            }
            stack.push(Frame {
                parent: chunks.len(),
                next: chunk.payload.start + 4,
                end: chunk.payload.end,
                alignment,
            });
        }
        chunks.push(chunk);
    }
    Ok(Container { source, chunks })
}

fn error(offset: usize, kind: ErrorKind) -> ContainerError {
    ContainerError { offset, kind }
}

fn read_chunk(
    source: &[u8],
    offset: usize,
    limit: usize,
    sibling_alignment: usize,
    parent: Option<usize>,
) -> Result<(Chunk, Option<usize>), ContainerError> {
    let tag_end = offset
        .checked_add(4)
        .ok_or_else(|| error(offset, ErrorKind::Overflow))?;
    if tag_end > limit {
        return Err(error(offset, ErrorKind::Truncated));
    }
    let tag: [u8; 4] = source[offset..tag_end].try_into().unwrap();
    let is_group = matches!(&tag[..3], b"FOR" | b"LIS" | b"CAT");
    if is_group && tag != *b"FOR8" && tag != *b"LIS8" {
        return Err(error(offset, ErrorKind::Unsupported));
    }
    let header_end = offset
        .checked_add(HEADER_BYTES)
        .ok_or_else(|| error(offset, ErrorKind::Overflow))?;
    if header_end > limit {
        return Err(error(offset, ErrorKind::Truncated));
    }
    let header = &source[offset..header_end];
    let (_, auxiliary) = be_u32::<_, nom::error::Error<_>>(&header[4..])
        .map_err(|_| error(offset, ErrorKind::Truncated))?;
    let (_, size) = be_u64::<_, nom::error::Error<_>>(&header[8..])
        .map_err(|_| error(offset, ErrorKind::Truncated))?;
    let size = usize::try_from(size).map_err(|_| error(offset, ErrorKind::Overflow))?;
    let payload_end = header_end
        .checked_add(size)
        .ok_or_else(|| error(offset, ErrorKind::Overflow))?;
    let padded_size = size
        .checked_add(sibling_alignment - 1)
        .map(|size| size & !(sibling_alignment - 1))
        .ok_or_else(|| error(offset, ErrorKind::Overflow))?;
    let next = header_end
        .checked_add(padded_size)
        .ok_or_else(|| error(offset, ErrorKind::Overflow))?;
    if next > limit {
        return Err(error(offset, ErrorKind::Truncated));
    }
    let (form, alignment) = if is_group {
        if size < 4 {
            return Err(error(offset, ErrorKind::Truncated));
        }
        let form = source[header_end..header_end + 4].try_into().unwrap();
        (
            Some(form),
            Some(group_alignment(tag, form).ok_or_else(|| error(offset, ErrorKind::Unsupported))?),
        )
    } else {
        (None, None)
    };
    Ok((
        Chunk {
            tag,
            form,
            auxiliary,
            parent,
            header: offset..header_end,
            payload: header_end..payload_end,
            padding: payload_end..next,
        },
        alignment,
    ))
}

pub(crate) fn group_alignment(tag: [u8; 4], form: [u8; 4]) -> Option<usize> {
    match (&tag, &form) {
        (b"FOR8", b"Maya") | (b"LIS8", b"CONS") => Some(4),
        (
            b"FOR8",
            b"AMBL" | b"CONN" | b"DCAM" | b"DMSH" | b"DMTI" | b"DPLM" | b"DSPL" | b"GPID" | b"HEAD"
            | b"NCRV" | b"NGEI" | b"OBST" | b"PCUU" | b"PSDM" | b"RBUM" | b"RCL3" | b"RLLK"
            | b"RLUM" | b"RNDL" | b"RNLM" | b"RPHO" | b"RPL2" | b"RTFT" | b"SCRP" | b"SDML"
            | b"SHAD" | b"SLCT" | b"SPTL" | b"STDS" | b"XFRM",
        ) => Some(8),
        (
            b"FOR8",
            [0x00, 0x08, 0x16, 0x20 | 0x22 | 0x30]
            | [0x00, 0x11, 0x5c, 0x00 | 0x03 | 0x04 | 0x85]
            | [0x00, 0x11, 0x5d, 0xc3]
            | [0x00, 0x81, 0x68, 0x40],
        ) => Some(8),
        _ => None,
    }
}
