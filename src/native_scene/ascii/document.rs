//! The read-back gate for authored Maya ASCII (#321). A scene that contains meshes is accepted
//! only when it is byte-identical to what the authored writer emits for the scene it decodes
//! to, so the reader never claims more of the Maya ASCII grammar than the writer produces.
use super::{
    AsciiError, ErrorKind, decode_with_uvs,
    write::{DefaultLinks, emit_with_links},
};
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
    let (sourced, uvs, links) =
        decode_with_uvs(source.clone(), true).map_err(AsciiDocumentError::Ascii)?;
    // The emitter writes one mesh per transform, so a scene with nodes but no shapes (curves)
    // can never be proved; skip the clone for it.
    if sourced.shapes.is_empty() && !sourced.nodes.is_empty() {
        return Ok(AsciiDecoded::Retained(sourced));
    }
    let mut candidate = sourced.clone();
    candidate.source = None;
    // A sourced scene cannot hold UVs, so they join the candidate only now. The comparison below
    // re-emits them, which is what admits a UV block: anything the writer would not emit refuses.
    for (shape, set) in sourced.shapes.iter().zip(uvs) {
        if let Some(set) = set {
            candidate.attach_authored_uv_set(shape.id(), set);
        }
    }
    // Any link selects the linked layout; the byte comparison then admits only the exact one
    // (every shape, in the writer's order), so a partial or duplicated set is refused.
    let layout = if links == 0 {
        DefaultLinks::Omit
    } else {
        DefaultLinks::Emit
    };
    // An empty scene the writer emits (a document whose last object was deleted) is an
    // authored document too: it must reopen the way it was saved.
    let proved = candidate.validate().is_ok()
        && emit_with_links(&candidate, layout).is_ok_and(|emitted| emitted.as_slice() == &*source);
    if !proved {
        return if sourced.shapes.is_empty() {
            Ok(AsciiDecoded::Retained(sourced))
        } else {
            Err(unsupported())
        };
    }
    if !adoption_supported {
        return if sourced.shapes.is_empty() {
            Ok(AsciiDecoded::Retained(sourced))
        } else {
            Err(AsciiDocumentError::AdoptionUnsupported)
        };
    }
    Ok(AsciiDecoded::Authored(AuthoredProjection(candidate)))
}

#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod uv_tests;
