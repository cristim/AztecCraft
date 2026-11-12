pub(crate) mod lex;

use std::fmt;

pub(crate) const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORK: usize = 50_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Malformed,
    Unsupported,
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsciiError {
    pub(crate) kind: ErrorKind,
    pub(crate) offset: Option<usize>,
}

impl AsciiError {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }
}

impl fmt::Display for AsciiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native ASCII scene {:?}", self.kind)?;
        if let Some(offset) = self.offset {
            write!(f, " at byte {offset}")?;
        }
        Ok(())
    }
}

impl std::error::Error for AsciiError {}

fn malformed(offset: usize) -> AsciiError {
    AsciiError {
        kind: ErrorKind::Malformed,
        offset: Some(offset),
    }
}

fn unsupported(offset: usize) -> AsciiError {
    AsciiError {
        kind: ErrorKind::Unsupported,
        offset: Some(offset),
    }
}

fn resource() -> AsciiError {
    AsciiError {
        kind: ErrorKind::ResourceLimit,
        offset: None,
    }
}

pub(crate) struct Work {
    used: usize,
}

impl Work {
    pub(crate) fn new() -> Self {
        Self { used: 0 }
    }

    pub(crate) fn charge(&mut self, amount: usize) -> Result<(), AsciiError> {
        self.used = self
            .used
            .checked_add(amount)
            .filter(|used| *used <= MAX_WORK)
            .ok_or_else(resource)?;
        Ok(())
    }
}
