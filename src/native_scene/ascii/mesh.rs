//! Mesh array attributes of the authored ASCII subset: `setAttr -s N ".vt[0:N-1]"` points,
//! `setAttr -s N ".ed[0:N-1]"` edges and `setAttr -s N ".fc[0:N-1]" -type "polyFaces"` faces
//! (`f n e...` rows only). Every shape is exactly what the authored writer emits; the
//! admission gate that compares re-emitted bytes backs this strictness up.
use super::{AsciiError, AttrFlags, Builder, Cursor, ErrorKind, error, mul, numeric, resource};
use crate::native_scene::{DirectedEdge, NativeEdge, NativeFace, NativeMesh};

/// Owner attribute bits for the mesh arrays already seen (bits 1..=4096 are taken).
const VERTICES: u32 = 8192;
const EDGES: u32 = 16384;
const FACES: u32 = 32768;
/// A face row is at least `f`, the count and three edge tokens.
const MIN_FACE_TOKENS: usize = 5;

#[derive(Default)]
pub(super) struct MeshParts {
    points: Option<Vec<[f32; 3]>>,
    edges: Option<Vec<NativeEdge>>,
    faces: Option<Vec<NativeFace>>,
}

/// Returns N for exactly `<name>[0:N-1]` with canonical digits and N equal to the `-s` size.
fn array_count(attribute: &str, name: &str, size: Option<usize>) -> Option<usize> {
    let digits = attribute
        .strip_prefix(name)?
        .strip_prefix("[0:")?
        .strip_suffix(']')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let last = digits.parse::<u32>().ok()?;
    (last.to_string() == digits).then_some(())?;
    let count = usize::try_from(last).ok()?.checked_add(1)?;
    (size == Some(count)).then_some(count)
}

impl Cursor<'_, '_> {
    /// The statement's remaining tokens must be exactly `rows * width`, so no count claim can
    /// outgrow the input that was already lexed and budgeted.
    fn exact_rows(&self, rows: usize, width: usize) -> Result<(), AsciiError> {
        let remaining = self.tokens.len().saturating_sub(self.position);
        if remaining != mul(rows, width)? {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        Ok(())
    }
    fn float32(&mut self) -> Result<f32, AsciiError> {
        let text = self.word()?;
        self.work.charge(mul(2, text.len())?)?;
        if !numeric(text.as_bytes()) {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        let value: f32 = text
            .parse()
            .map_err(|_| error(ErrorKind::Malformed, self.offset))?;
        if !value.is_finite() {
            return Err(error(ErrorKind::Malformed, self.offset));
        }
        Ok(value)
    }
    /// Decodes a polyFaces edge token: `t >= 0` is edge `t`, `t < 0` is edge `-t-1` reversed. The
    /// arithmetic is in i64 so `i32::MIN` cannot overflow, and the edge must exist.
    fn face_edge(&mut self, edges: usize) -> Result<DirectedEdge, AsciiError> {
        let token = i64::from(self.integer()?);
        let (id, reversed) = if token < 0 {
            (-token - 1, true)
        } else {
            (token, false)
        };
        let edge = usize::try_from(id)
            .ok()
            .filter(|edge| *edge < edges)
            .ok_or_else(|| error(ErrorKind::Malformed, self.offset))?;
        Ok(DirectedEdge { edge, reversed })
    }
    fn vertex(&mut self) -> Result<usize, AsciiError> {
        usize::try_from(self.integer()?).map_err(|_| error(ErrorKind::Malformed, self.offset))
    }
}

impl Builder<'_, '_> {
    pub(super) fn mesh_attribute(
        &mut self,
        owner: usize,
        index: usize,
        flags: &AttrFlags<'_>,
        c: &mut Cursor<'_, '_>,
        attribute: &str,
    ) -> Result<(), AsciiError> {
        if !self.units || flags.keyability {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let plain = flags.value_type.is_none();
        if let (true, Some(rows)) = (plain, array_count(attribute, ".vt", flags.size)) {
            c.exact_rows(rows, 3)?;
            self.mark(owner, VERTICES, c)?;
            let mut points = self.budget.vector(rows).map_err(|_| resource())?;
            for _ in 0..rows {
                points.push([c.float32()?, c.float32()?, c.float32()?]);
            }
            self.mesh_parts[index].points = Some(points);
        } else if let (true, Some(rows)) = (plain, array_count(attribute, ".ed", flags.size)) {
            c.exact_rows(rows, 3)?;
            self.mark(owner, EDGES, c)?;
            let mut edges = self.budget.vector(rows).map_err(|_| resource())?;
            for _ in 0..rows {
                let endpoints = [c.vertex()?, c.vertex()?];
                // The third value is Maya's smooth flag; 0 (hard) is the only value written.
                if c.word()? != "0" {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                edges.push(NativeEdge {
                    endpoints,
                    smooth: Some(false),
                });
            }
            self.mesh_parts[index].edges = Some(edges);
        } else if let (Some("polyFaces"), Some(rows)) =
            (flags.value_type, array_count(attribute, ".fc", flags.size))
        {
            self.faces(owner, index, rows, c)?;
        } else {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        c.done()
    }

    fn faces(
        &mut self,
        owner: usize,
        index: usize,
        rows: usize,
        c: &mut Cursor<'_, '_>,
    ) -> Result<(), AsciiError> {
        // Faces name edges by index, so the edge array must already be known.
        let edges = self.mesh_parts[index]
            .edges
            .as_ref()
            .map(Vec::len)
            .ok_or_else(|| error(ErrorKind::Unsupported, c.offset))?;
        let remaining = c.tokens.len().saturating_sub(c.position);
        if mul(rows, MIN_FACE_TOKENS)? > remaining {
            return Err(error(ErrorKind::Malformed, c.offset));
        }
        self.mark(owner, FACES, c)?;
        let mut faces = self.budget.vector(rows).map_err(|_| resource())?;
        for _ in 0..rows {
            // Only `f` rows: holes (`h`), UV (`mu`, `mf`, `mh`) and colour (`mc`) are refused.
            if c.word()? != "f" {
                return Err(error(ErrorKind::Unsupported, c.offset));
            }
            let corners = c.count()?;
            let left = c.tokens.len().saturating_sub(c.position);
            if corners < 3 || corners > left {
                return Err(error(ErrorKind::Malformed, c.offset));
            }
            let mut outer = self.budget.vector(corners).map_err(|_| resource())?;
            for _ in 0..corners {
                outer.push(c.face_edge(edges)?);
            }
            faces.push(NativeFace {
                outer,
                holes: Vec::new(),
            });
        }
        self.mesh_parts[index].faces = Some(faces);
        Ok(())
    }

    /// Replaces each placeholder mesh with the validated mesh its arrays describe.
    pub(super) fn finish_meshes(&mut self) -> Result<(), AsciiError> {
        for (shape, parts) in self.scene.shapes.iter_mut().zip(self.mesh_parts.drain(..)) {
            shape.mesh = NativeMesh::from_parts(
                parts.points.unwrap_or_default(),
                parts.edges.unwrap_or_default(),
                parts.faces.unwrap_or_default(),
                Vec::new(),
                Some(false),
            )
            .map_err(|_| AsciiError {
                kind: ErrorKind::Malformed,
                offset: None,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
