//! OpenUSD ASCII (`.usda`) export of authored polygon meshes as `UsdGeomMesh` prims.
//!
//! Units: AztecCraft scene units are centimeters with +Y up, so the layer states
//! `metersPerUnit = 0.01` and `upAxis = "Y"` and points are written as authored, with no scaling.
//! Each point is the f64 world-space position rounded once to f32 and printed with Rust's
//! shortest round-trip formatting, so reading the text back as an f32 yields the same bits.
//! Every exported instance becomes one flat `def Mesh` under `def Xform "World"` in world space,
//! keeping its native polygon arity. UVs, materials, normals, animation, instancing and
//! hierarchy are not written: a shape with session UVs or an assigned session material is
//! refused rather than exported without them.
//! Spec: <https://openusd.org/release/api/class_usd_geom_mesh.html> and
//! <https://openusd.org/release/api/group___usd_geom_linear_units__group.html>.

use crate::export_write::{StagedWriteError, write_new_staged};
use crate::native_scene::{NativeError, NativeScene};
use crate::view_scene::ViewInstance;
use glam::DVec3;
use std::{
    collections::HashSet,
    fmt::{self, Write as _},
    io,
    path::{Path, PathBuf},
};

/// Largest `.usda` text written, in bytes.
pub const MAX_TEXT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsdSummary {
    pub objects: usize,
    pub vertices: usize,
    pub faces: usize,
}

#[derive(Debug)]
pub enum UsdError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Native(NativeError),
    /// The path does not end in `.usda`.
    UnknownExtension,
    NothingToExport,
    /// `may_install` refused; nothing was installed.
    Cancelled,
    NonFinite,
    TooLarge,
    /// A node name is not a unique USD identifier (a scene invariant violation).
    InvalidName,
    /// A face corner indexes past the shape's points.
    InvalidTopology,
    /// A shape has session UVs, which this export does not write.
    SessionUvs,
    /// A shape has an assigned session material, which this export does not write.
    SessionMaterial,
}

impl fmt::Display for UsdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "export USD {}: {source}", path.display()),
            Self::Native(source) => write!(f, "invalid scene for USD export: {source}"),
            Self::UnknownExtension => f.write_str("USD export path must end in .usda"),
            Self::NothingToExport => f.write_str("no displayed polygon geometry to export"),
            Self::Cancelled => f.write_str("export cancelled before the file was installed"),
            Self::NonFinite => f.write_str("a position is not finite as a 32-bit float"),
            Self::TooLarge => f.write_str("the export exceeds the USD text size limit"),
            Self::InvalidTopology => f.write_str("a face references a missing point"),
            Self::InvalidName => f.write_str("a node name is not a unique USD prim name"),
            Self::SessionUvs => f.write_str(
                "a shape has session UVs, which USD export does not write yet; use Delete UVs first",
            ),
            Self::SessionMaterial => f.write_str(
                "a shape has an assigned session material, which USD export does not write yet",
            ),
        }
    }
}

impl std::error::Error for UsdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Native(source) => Some(source),
            _ => None,
        }
    }
}

/// Only the ASCII encoding is written, so `.usd` (which may hold either) is not accepted.
pub fn is_usda_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("usda"))
}

/// Scene node names are admitted as `[A-Za-z_][A-Za-z0-9_]*` and are unique scene-wide, so they
/// are valid, distinct USD prim names. This checks that invariant instead of rewriting names.
fn prim_name(name: &str, taken: &mut HashSet<String>) -> Result<String, UsdError> {
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_');
    if valid && taken.insert(name.to_owned()) {
        Ok(name.to_owned())
    } else {
        Err(UsdError::InvalidName)
    }
}

struct Mesh {
    name: String,
    points: Vec<[f32; 3]>,
    counts: Vec<usize>,
    indices: Vec<usize>,
}

fn collect(scene: &NativeScene, instances: &[&ViewInstance]) -> Result<Vec<Mesh>, UsdError> {
    let mut meshes = Vec::new();
    let mut taken = HashSet::new();
    for instance in instances {
        if scene.uv_set(instance.shape()).is_some() {
            return Err(UsdError::SessionUvs);
        }
        let shape = scene.shape(instance.shape()).map_err(UsdError::Native)?;
        if scene.has_assigned_session_material(shape) {
            return Err(UsdError::SessionMaterial);
        }
        let mesh = shape.mesh();
        if instance.faces().is_empty() || mesh.point_count() == 0 {
            continue;
        }
        if i32::try_from(mesh.point_count()).is_err() {
            return Err(UsdError::TooLarge);
        }
        let world = instance.world_matrix();
        let mut points = Vec::new();
        points
            .try_reserve_exact(mesh.point_count())
            .map_err(|_| UsdError::TooLarge)?;
        for index in 0..mesh.point_count() {
            let local =
                DVec3::from_array(mesh.effective_point_cm(index).map_err(UsdError::Native)?);
            let p = world.transform_point3(local);
            let point = [p.x as f32, p.y as f32, p.z as f32];
            if !point.iter().all(|c| c.is_finite()) {
                return Err(UsdError::NonFinite);
            }
            points.push(point);
        }
        // A mirrored instance reverses every face so it still faces outward under USD's default
        // right-handed orientation.
        let mirrored = world.determinant() < 0.0;
        let (mut counts, mut indices) = (Vec::new(), Vec::new());
        for face in instance.faces() {
            let corners = face.corners();
            if corners.iter().any(|&c| c >= points.len()) {
                return Err(UsdError::InvalidTopology);
            }
            counts.try_reserve(1).map_err(|_| UsdError::TooLarge)?;
            indices
                .try_reserve(corners.len())
                .map_err(|_| UsdError::TooLarge)?;
            counts.push(corners.len());
            if mirrored {
                indices.extend(corners.iter().rev());
            } else {
                indices.extend(corners);
            }
        }
        let node = scene.node(instance.transform()).map_err(UsdError::Native)?;
        meshes.push(Mesh {
            name: prim_name(node.name(), &mut taken)?,
            points,
            counts,
            indices,
        });
    }
    Ok(meshes)
}

fn push_list<T>(
    out: &mut String,
    limit: usize,
    items: &[T],
    mut write: impl FnMut(&mut String, &T) -> fmt::Result,
) -> Result<(), UsdError> {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write(out, item).map_err(|_| UsdError::TooLarge)?;
        if out.len() > limit {
            return Err(UsdError::TooLarge);
        }
    }
    out.push(']');
    Ok(())
}

fn render(meshes: &[Mesh], limit: usize) -> Result<String, UsdError> {
    let mut out = String::from(
        "#usda 1.0\n(\n    defaultPrim = \"World\"\n    metersPerUnit = 0.01\n    upAxis = \"Y\"\n)\n\ndef Xform \"World\"\n{\n",
    );
    for (n, mesh) in meshes.iter().enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
        for point in &mesh.points {
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        let _ = write!(out, "    def Mesh \"{}\"\n    {{\n", mesh.name);
        out.push_str("        int[] faceVertexCounts = ");
        push_list(&mut out, limit, &mesh.counts, |o, v| write!(o, "{v}"))?;
        out.push_str("\n        int[] faceVertexIndices = ");
        push_list(&mut out, limit, &mesh.indices, |o, v| write!(o, "{v}"))?;
        let _ = write!(
            out,
            "\n        float3[] extent = [({}, {}, {}), ({}, {}, {})]\n        point3f[] points = ",
            min[0], min[1], min[2], max[0], max[1], max[2]
        );
        push_list(&mut out, limit, &mesh.points, |o, p| {
            write!(o, "({}, {}, {})", p[0], p[1], p[2])
        })?;
        out.push_str("\n        uniform token subdivisionScheme = \"none\"\n    }\n");
    }
    out.push_str("}\n");
    if out.len() > limit {
        return Err(UsdError::TooLarge);
    }
    Ok(out)
}

/// Writes the given prepared mesh instances to a new `.usda` file in world space (centimeters,
/// +Y up). The destination must not exist; nothing is written on refusal. `may_install` is asked
/// before the staged file is created and again before it is given the destination's name; a
/// refusal returns [`UsdError::Cancelled`] and leaves nothing behind.
pub fn export_native_new(
    path: &Path,
    scene: &NativeScene,
    instances: &[&ViewInstance],
    may_install: &dyn Fn() -> bool,
) -> Result<UsdSummary, UsdError> {
    if !is_usda_path(path) {
        return Err(UsdError::UnknownExtension);
    }
    let meshes = collect(scene, instances)?;
    if meshes.is_empty() {
        return Err(UsdError::NothingToExport);
    }
    let summary = UsdSummary {
        objects: meshes.len(),
        vertices: meshes.iter().map(|m| m.points.len()).sum(),
        faces: meshes.iter().map(|m| m.counts.len()).sum(),
    };
    let text = render(&meshes, MAX_TEXT_BYTES)?;
    write_new_staged(path, text.as_bytes(), may_install).map_err(|error| match error {
        StagedWriteError::Cancelled => UsdError::Cancelled,
        StagedWriteError::Io { path, source } => UsdError::Io { path, source },
    })?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Mesh {
        Mesh {
            name: "a".into(),
            points: vec![[0.0; 3]; 3],
            counts: vec![3],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn render_refuses_text_over_the_limit() {
        let full = render(&[triangle()], MAX_TEXT_BYTES).unwrap();
        assert!(render(&[triangle()], full.len()).is_ok());
        assert!(matches!(
            render(&[triangle()], full.len() - 1),
            Err(UsdError::TooLarge)
        ));
        assert!(matches!(render(&[triangle()], 20), Err(UsdError::TooLarge)));
    }

    #[test]
    fn prim_names_must_be_unique_identifiers() {
        let mut taken = HashSet::new();
        assert_eq!(prim_name("pCube1", &mut taken).unwrap(), "pCube1");
        for bad in ["pCube1", "", "1a", "a-b", "a b"] {
            assert!(matches!(
                prim_name(bad, &mut taken),
                Err(UsdError::InvalidName)
            ));
        }
    }
}
