mod admission;
mod appearance;
mod binding;
mod encode;
mod write;
pub use encode::encode;
#[cfg(test)]
mod tests;

use super::{NativeError, NativeScene};
use crate::maya_binary::{container, semantic};
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Malformed,
    Unsupported,
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinaryError {
    kind: ErrorKind,
    offset: Option<usize>,
}

impl BinaryError {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn offset(&self) -> Option<usize> {
        self.offset
    }
}

impl fmt::Display for BinaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "binary scene {:?}", self.kind)?;
        if let Some(offset) = self.offset {
            write!(formatter, " at byte {offset}")?;
        }
        Ok(())
    }
}

impl std::error::Error for BinaryError {}

fn fail(kind: ErrorKind, offset: usize) -> BinaryError {
    BinaryError {
        kind,
        offset: Some(offset),
    }
}

fn resource() -> BinaryError {
    BinaryError {
        kind: ErrorKind::ResourceLimit,
        offset: None,
    }
}

fn canonical(error: NativeError, offset: Option<usize>) -> BinaryError {
    BinaryError {
        kind: match error {
            NativeError::ResourceLimit => ErrorKind::ResourceLimit,
            NativeError::UnsupportedEdit => ErrorKind::Unsupported,
            _ => ErrorKind::Malformed,
        },
        offset: if error == NativeError::ResourceLimit {
            None
        } else {
            offset
        },
    }
}

impl From<container::ContainerError> for BinaryError {
    fn from(error: container::ContainerError) -> Self {
        use container::ErrorKind as K;
        Self {
            kind: match error.kind() {
                K::Unsupported => ErrorKind::Unsupported,
                K::ByteLimit | K::ChunkLimit | K::DepthLimit => ErrorKind::ResourceLimit,
                _ => ErrorKind::Malformed,
            },
            offset: (error.kind() != K::ByteLimit).then_some(error.offset()),
        }
    }
}

impl From<semantic::BinaryError> for BinaryError {
    fn from(error: semantic::BinaryError) -> Self {
        Self {
            kind: match error.kind {
                semantic::ErrorKind::Unsupported => ErrorKind::Unsupported,
                semantic::ErrorKind::Resource => ErrorKind::ResourceLimit,
                _ => ErrorKind::Malformed,
            },
            offset: error.offset,
        }
    }
}

pub fn decode(source: Arc<[u8]>) -> Result<NativeScene, BinaryError> {
    decode_with_retained_bytes(source, 0, 0)
}

fn decode_with_retained_bytes(
    source: Arc<[u8]>,
    retained_bytes: usize,
    retained_appearance_bytes: usize,
) -> Result<NativeScene, BinaryError> {
    admission::Budget::new(
        source
            .len()
            .checked_add(retained_bytes)
            .and_then(|bytes| bytes.checked_add(retained_appearance_bytes))
            .ok_or_else(resource)?,
    )?;
    let container = container::inspect(source)?;
    let facts = semantic::decode_records(&container)?;
    binding::construct(
        &container,
        &facts,
        retained_bytes,
        retained_appearance_bytes,
    )
}
