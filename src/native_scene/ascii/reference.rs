//! `file -r ...` reference declarations. See `native_scene::references`.

use super::*;
use crate::native_scene::references::{
    MAX_REFERENCE_PATH_BYTES, MAX_REFERENCES, ReferenceDeclaration, ReferencePathKind, path_kind,
    printable, text_ok,
};

impl Builder<'_, '_> {
    /// `file -r [-ns s] [-rfn s] [-rpr s] [-typ s] [-dr b] [-rdi n] "path"`: each flag at most
    /// once, the path last. `-op`, `-lrd`, `-lck`, `-iv` and everything else the pinned page does
    /// not define for `.ma` refuse the whole candidate.
    pub(super) fn file_reference(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), AsciiError> {
        let unsupported = |c: &Cursor<'_, '_>| error(ErrorKind::Unsupported, c.offset);
        let mut seen = 0u8;
        let mut r = ReferenceDeclaration {
            path_kind: ReferencePathKind::Relative,
            path: None,
            namespace: None,
            reference_node: None,
            renaming_prefix: None,
            file_type: None,
            deferred: None,
            depth_info: None,
        };
        while c.peek()?.is_some_and(|(kind, _)| kind == TokenKind::Word) {
            let bit = match c.word()? {
                "-r" | "-reference" => 1,
                "-ns" | "-namespace" => 2,
                "-rfn" | "-referenceNode" => 4,
                "-rpr" | "-renamingPrefix" => 8,
                "-typ" | "-type" => 16,
                "-dr" | "-deferReference" => 32,
                "-rdi" | "-referenceDepthInfo" => 64,
                _ => return Err(unsupported(c)),
            };
            if seen & bit != 0 {
                return Err(unsupported(c));
            }
            seen |= bit;
            match bit {
                1 => {}
                32 => r.deferred = Some(c.boolean()?),
                64 => {
                    let text = c.word()?;
                    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(error(ErrorKind::Malformed, c.offset));
                    }
                    r.depth_info = Some(
                        text.parse()
                            .map_err(|_| error(ErrorKind::Malformed, c.offset))?,
                    );
                }
                _ => {
                    let (text, _) = c.literal()?;
                    if !text_ok(text) {
                        return Err(unsupported(c));
                    }
                    let owned = Some(self.budget.string(text).map_err(|_| resource())?);
                    match bit {
                        2 => r.namespace = owned,
                        4 => r.reference_node = owned,
                        8 => r.renaming_prefix = owned,
                        _ => r.file_type = owned,
                    }
                }
            }
        }
        if seen & 1 == 0 {
            return Err(unsupported(c));
        }
        let (path, _) = c.literal()?;
        c.done()?;
        if path.len() > MAX_REFERENCE_PATH_BYTES {
            return Err(resource());
        }
        if path.is_empty() || !printable(path) {
            return Err(unsupported(c));
        }
        r.path_kind = path_kind(path);
        if r.path_kind == ReferencePathKind::Relative {
            r.path = Some(self.budget.string(path).map_err(|_| resource())?);
        }
        if self.scene.references.len() >= MAX_REFERENCES {
            return Err(resource());
        }
        self.scene.references.push(r);
        Ok(())
    }
}
