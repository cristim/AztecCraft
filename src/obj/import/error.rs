//! Typed, content-free errors of the OBJ reader: a keyword class and a 1-based line number only.

use super::{MAX_COORD, MAX_FILE_BYTES, MAX_LINE_BYTES};
use std::{fmt, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxKind {
    VertexNeedsThreeNumbers,
    BadNumber,
    BadFaceCorner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedKind {
    FreeForm,
    UnknownKeyword,
    OverlappingGroups,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Vertices,
    Faces,
    Corners,
    CornersPerFace,
    Objects,
    NameBytes,
    Allocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjImportError {
    Io(io::ErrorKind),
    NotRegularFile,
    TooLarge,
    LineTooLong { line: usize },
    CrOnlyLineEndings { line: usize },
    InvalidUtf8 { line: usize },
    Syntax { line: usize, what: SyntaxKind },
    IndexOutOfRange { line: usize },
    DegenerateFace { line: usize },
    CoordinateOutOfRange { line: usize },
    Unsupported { line: usize, what: UnsupportedKind },
    LimitExceeded(Limit),
    NoFaces,
}

impl fmt::Display for ObjImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(kind) => write!(f, "could not read the OBJ file ({kind})"),
            Self::NotRegularFile => f.write_str("the OBJ path is not a regular file"),
            Self::TooLarge => write!(f, "the OBJ file is over {MAX_FILE_BYTES} bytes"),
            Self::LineTooLong { line } => {
                write!(f, "line {line} is over {MAX_LINE_BYTES} bytes")
            }
            Self::CrOnlyLineEndings { line } => write!(
                f,
                "line {line}: CR-only line endings are not supported; convert to LF or CRLF"
            ),
            Self::InvalidUtf8 { line } => write!(f, "line {line} is not valid UTF-8"),
            Self::Syntax { line, what } => {
                let what = match what {
                    SyntaxKind::VertexNeedsThreeNumbers => "a vertex needs three numbers",
                    SyntaxKind::BadNumber => "a number is malformed",
                    SyntaxKind::BadFaceCorner => "a face corner is malformed",
                };
                write!(f, "line {line}: {what}")
            }
            Self::IndexOutOfRange { line } => write!(
                f,
                "line {line}: a face index is zero, out of range or refers to a later record"
            ),
            Self::DegenerateFace { line } => write!(
                f,
                "line {line}: a face has fewer than 3 corners or repeats a vertex; triangulate the model and export again"
            ),
            Self::CoordinateOutOfRange { line } => write!(
                f,
                "line {line}: a coordinate is not finite or is beyond {MAX_COORD:e}"
            ),
            Self::Unsupported { line, what } => {
                let what = match what {
                    UnsupportedKind::FreeForm => "free-form geometry is not supported",
                    UnsupportedKind::UnknownKeyword => "unknown record keyword",
                    UnsupportedKind::OverlappingGroups => {
                        "a group line with several names (overlapping groups) is not supported"
                    }
                };
                write!(f, "line {line}: {what}")
            }
            Self::LimitExceeded(limit) => write!(f, "the OBJ file exceeds the {limit:?} limit"),
            Self::NoFaces => f.write_str("the OBJ file has no face"),
        }
    }
}

impl std::error::Error for ObjImportError {}
