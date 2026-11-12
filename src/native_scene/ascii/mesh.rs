//! Mesh array attributes of the authored ASCII subset: `setAttr -s N ".vt[0:N-1]"` points,
//! `setAttr -s N ".ed[0:N-1]"` edges and `setAttr -s N ".fc[0:N-1]" -type "polyFaces"` faces
//! (`f n e...` rows, each optionally followed by `mu 0 n ids` with the running ids), plus the
//! PROVISIONAL UV statements the writer emits after them (`.uvst[0].uvsn`, `.uvst[0].uvsp`,
//! `.cuvs`, see `.project-docs/uv-ascii-format.md`). Every shape is exactly what the authored
//! writer emits; the admission gate that compares re-emitted bytes backs this strictness up.
use super::{
    AsciiError, AttrFlags, Builder, Cursor, ErrorKind, TokenKind, add, error, mul, numeric,
    resource,
};
use crate::native_scene::{
    DEFAULT_UV_SET, DirectedEdge, NativeEdge, NativeFace, NativeMesh, UvSet, admission::Budget,
};

/// Owner attribute bits for the mesh arrays already seen (bits 1..=4096 are taken).
const VERTICES: u32 = 8192;
const EDGES: u32 = 16384;
const FACES: u32 = 32768;
const UV_NAME: u32 = 65536;
const UV_POINTS: u32 = 131072;
const UV_CURRENT: u32 = 262144;
/// A face row is at least `f`, the count and three edge tokens.
const MIN_FACE_TOKENS: usize = 5;

#[derive(Default)]
pub(super) struct MeshParts {
    points: Option<Vec<[f32; 3]>>,
    edges: Option<Vec<NativeEdge>>,
    faces: Option<Vec<NativeFace>>,
    uv: UvParts,
}

/// What the UV statements of one shape said. `corners` is `Some` once `.fc` rows carried `mu`.
#[derive(Default)]
struct UvParts {
    corners: Option<usize>,
    name: bool,
    current: bool,
    points: Option<Vec<[f32; 2]>>,
}

impl UvParts {
    /// The shape's `map1` once every statement of the block was seen, or `None` without `mu` rows.
    fn finish(self, mesh: &NativeMesh, budget: &mut Budget) -> Result<Option<UvSet>, AsciiError> {
        if self.corners.is_none() {
            return Ok(None);
        }
        let (true, true, Some(points)) = (self.name, self.current, self.points) else {
            return Err(AsciiError {
                kind: ErrorKind::Malformed,
                offset: None,
            });
        };
        let mut points = points.into_iter();
        let mut faces = budget.vector(mesh.faces().len()).map_err(|_| resource())?;
        for face in mesh.faces() {
            let mut corners = budget.vector(face.outer().len()).map_err(|_| resource())?;
            corners.extend(
                points
                    .by_ref()
                    .take(face.outer().len())
                    .map(|[u, v]| [f64::from(u), f64::from(v)]),
            );
            faces.push(corners);
        }
        UvSet::from_corners(mesh, faces)
            .map(Some)
            .map_err(|_| AsciiError {
                kind: ErrorKind::Malformed,
                offset: None,
            })
    }
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
    /// Decodes a polyFaces edge token: `t >= 0` is edge `t`, `t < 0` is edge `-t-1` reversed
    /// (PROVISIONAL: the setAttr page says only that a negative id is reversed, which cannot
    /// express edge 0; the one's-complement form is pinned by the writer and its tests, see
    /// section 2 of the #321 plan). The arithmetic is in i64 so `i32::MIN` cannot overflow, and
    /// the edge must exist.
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
        } else if let (Some("string"), ".uvst[0].uvsn" | ".cuvs") = (flags.value_type, attribute) {
            self.uv_name(owner, index, attribute, flags.size, c)?;
        } else if let (Some("float2"), Some(rows)) = (
            flags.value_type,
            array_count(attribute, ".uvst[0].uvsp", flags.size),
        ) {
            self.uv_points(owner, index, rows, c)?;
        } else {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        c.done()
    }

    /// The set name statements: exactly `"map1"`, and only after `.fc` rows that carried `mu`.
    fn uv_name(
        &mut self,
        owner: usize,
        index: usize,
        attribute: &str,
        size: Option<usize>,
        c: &mut Cursor<'_, '_>,
    ) -> Result<(), AsciiError> {
        if size.is_some() || self.mesh_parts[index].uv.corners.is_none() {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        if c.literal()?.0 != DEFAULT_UV_SET {
            return Err(error(ErrorKind::Unsupported, c.offset));
        }
        let (bit, seen) = if attribute == ".cuvs" {
            (UV_CURRENT, &mut self.mesh_parts[index].uv.current)
        } else {
            (UV_NAME, &mut self.mesh_parts[index].uv.name)
        };
        *seen = true;
        self.mark(owner, bit, c)
    }

    /// `uvsp`: one `u v` row per `mu` id, so the count is the face corner total.
    fn uv_points(
        &mut self,
        owner: usize,
        index: usize,
        rows: usize,
        c: &mut Cursor<'_, '_>,
    ) -> Result<(), AsciiError> {
        let Some(corners) = self.mesh_parts[index].uv.corners else {
            return Err(error(ErrorKind::Unsupported, c.offset));
        };
        if rows != corners {
            return Err(error(ErrorKind::Malformed, c.offset));
        }
        c.exact_rows(rows, 2)?;
        self.mark(owner, UV_POINTS, c)?;
        let mut points = self.budget.vector(rows).map_err(|_| resource())?;
        for _ in 0..rows {
            points.push([c.float32()?, c.float32()?]);
        }
        self.mesh_parts[index].uv.points = Some(points);
        Ok(())
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
        let (mut next_uv, mut with_mu) = (0usize, None);
        for _ in 0..rows {
            // Only `f` rows, each with the single running `mu 0` block or none (all rows alike):
            // holes (`h`), `mf`, `mh` and colour (`mc`) are refused.
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
            let has_mu = matches!(c.peek()?, Some((TokenKind::Word, "mu")));
            if *with_mu.get_or_insert(has_mu) != has_mu {
                return Err(error(ErrorKind::Malformed, c.offset));
            }
            if has_mu {
                c.word()?;
                if c.count()? != 0 {
                    return Err(error(ErrorKind::Unsupported, c.offset));
                }
                if c.count()? != corners || corners > c.tokens.len().saturating_sub(c.position) {
                    return Err(error(ErrorKind::Malformed, c.offset));
                }
                for _ in 0..corners {
                    if c.count()? != next_uv {
                        return Err(error(ErrorKind::Unsupported, c.offset));
                    }
                    next_uv = add(next_uv, 1)?;
                }
            }
            faces.push(NativeFace {
                outer,
                holes: Vec::new(),
            });
        }
        self.mesh_parts[index].uv.corners = (with_mu == Some(true)).then_some(next_uv);
        self.mesh_parts[index].faces = Some(faces);
        Ok(())
    }

    /// Replaces each placeholder mesh with the validated mesh its arrays describe.
    /// Returns each shape's UV set (parallel to the scene's shapes); it cannot live in the sourced
    /// scene, so the caller attaches it to the source-free candidate.
    pub(super) fn finish_meshes(&mut self) -> Result<Vec<Option<UvSet>>, AsciiError> {
        let mut sets = self
            .budget
            .vector(self.scene.shapes.len())
            .map_err(|_| resource())?;
        for (shape, parts) in self.scene.shapes.iter_mut().zip(self.mesh_parts.drain(..)) {
            let uv = parts.uv;
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
            sets.push(uv.finish(&shape.mesh, self.budget)?);
        }
        Ok(sets)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod uv_tests;
