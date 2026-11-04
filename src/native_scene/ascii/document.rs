//! The read-back gate for authored Maya ASCII (#321). A scene that contains meshes is accepted
//! only when it is byte-identical to what the authored writer emits for the scene it decodes
//! to, so the reader never claims more of the Maya ASCII grammar than the writer produces.
use super::{AsciiError, ErrorKind, decode_inner, write::emit_geometry_prefix};
use crate::native_scene::NativeScene;
use std::{fmt, sync::Arc};

/// A source-free scene proved to re-emit the exact bytes it was decoded from. Only
/// [`decode_document`] constructs it, so holding one is the proof that adopting the file as an
/// authored document loses nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredProjection(NativeScene);

impl AuthoredProjection {
    pub fn scene(&self) -> &NativeScene {
        &self.0
    }
    pub fn into_scene(self) -> NativeScene {
        self.0
    }
}

/// What an ASCII document decodes to.
#[derive(Clone, Debug, PartialEq)]
pub enum AsciiDecoded {
    /// A mesh-free scene that retains its source bytes and stays read-only.
    Retained(NativeScene),
    /// A mesh scene proved byte-identical to the authored writer's output.
    Authored(AuthoredProjection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsciiDocumentError {
    Ascii(AsciiError),
    /// The file passes the gate, but this platform cannot adopt it as an authored document.
    AdoptionUnsupported,
}

impl fmt::Display for AsciiDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ascii(error) => error.fmt(f),
            Self::AdoptionUnsupported => {
                f.write_str("Maya ASCII scenes with meshes can be opened only on macOS and Linux")
            }
        }
    }
}

impl std::error::Error for AsciiDocumentError {}

fn unsupported() -> AsciiDocumentError {
    AsciiDocumentError::Ascii(AsciiError {
        kind: ErrorKind::Unsupported,
        offset: None,
    })
}

/// Decodes an ASCII document. `adoption_supported` is the caller's platform decision (the
/// authored writer's Save is Unix-only); when false, an otherwise valid mesh file is refused
/// with [`AsciiDocumentError::AdoptionUnsupported`].
pub fn decode_document(
    source: Arc<[u8]>,
    adoption_supported: bool,
) -> Result<AsciiDecoded, AsciiDocumentError> {
    let refused = match decode_inner(source.clone(), false) {
        Ok(scene) => return Ok(AsciiDecoded::Retained(scene)),
        Err(error) => error,
    };
    // Only a refusal at a mesh statement can be a gated file; any other refusal stands.
    let at_mesh = refused
        .offset()
        .and_then(|offset| source.get(offset..))
        .is_some_and(|rest| rest.starts_with(b"createNode mesh"));
    if !at_mesh {
        return Err(AsciiDocumentError::Ascii(refused));
    }
    let mut scene = decode_inner(source.clone(), true).map_err(AsciiDocumentError::Ascii)?;
    scene.source = None;
    scene.validate().map_err(|_| unsupported())?;
    let emitted = emit_geometry_prefix(&scene).map_err(|_| unsupported())?;
    if emitted.as_slice() != &*source {
        return Err(unsupported());
    }
    if !adoption_supported {
        return Err(AsciiDocumentError::AdoptionUnsupported);
    }
    Ok(AsciiDecoded::Authored(AuthoredProjection(scene)))
}

#[cfg(test)]
mod tests;
