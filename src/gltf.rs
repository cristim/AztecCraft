//! glTF 2.0 export of authored polygon meshes (`.gltf` with an embedded base64 buffer, or `.glb`).
//!
//! Units are meters, as the specification requires: AztecCraft centimeters are divided by 100 in
//! f64 and then rounded once to f32. Axes are written as authored (+Y up). Each exported instance
//! becomes one flat node with one triangle primitive in world space. NORMAL is omitted because
//! native meshes carry none, and glTF 2.0 makes clients compute flat normals when it is absent.
//! UVs, materials, animation and skins are unsupported: a shape that has session UVs or an
//! assigned session material is refused rather than exported without them.
//! Spec: <https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html>.

use crate::export_write::{StagedWriteError, write_new_staged};
use crate::native_scene::{NativeError, NativeScene};
use crate::view_scene::ViewInstance;
use glam::DVec3;
use serde_json::{Value, json};
use std::{
    fmt, io,
    path::{Path, PathBuf},
};

/// Largest binary buffer written, in bytes.
pub const MAX_BUFFER_BYTES: usize = 256 * 1024 * 1024;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// Container chosen by the destination extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfFormat {
    /// JSON with the buffer embedded as a base64 data URI.
    Gltf,
    /// Binary container with a JSON chunk and a BIN chunk.
    Glb,
}

impl GltfFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension.eq_ignore_ascii_case("gltf") {
            Some(Self::Gltf)
        } else if extension.eq_ignore_ascii_case("glb") {
            Some(Self::Glb)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GltfSummary {
    pub objects: usize,
    pub vertices: usize,
    pub triangles: usize,
}

#[derive(Debug)]
pub enum GltfError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Native(NativeError),
    /// The path does not end in `.gltf` or `.glb`.
    UnknownExtension,
    NothingToExport,
    /// `may_install` refused; nothing was installed.
    Cancelled,
    NonFinite,
    TooLarge,
    /// A shape has session UVs, which this export does not write.
    SessionUvs,
    /// A shape has an assigned session material, which this export does not write.
    SessionMaterial,
}

impl fmt::Display for GltfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "export glTF {}: {source}", path.display()),
            Self::Native(source) => write!(f, "invalid scene for glTF export: {source}"),
            Self::UnknownExtension => f.write_str("glTF export path must end in .gltf or .glb"),
            Self::NothingToExport => f.write_str("no displayed polygon geometry to export"),
            Self::Cancelled => f.write_str("export cancelled before the file was installed"),
            Self::NonFinite => f.write_str("a position is not finite as a 32-bit float in meters"),
            Self::TooLarge => f.write_str("the export exceeds the binary buffer limit"),
            Self::SessionUvs => f.write_str(
                "a shape has session UVs, which glTF export does not write yet; use Delete UVs first",
            ),
            Self::SessionMaterial => f.write_str(
                "a shape has an assigned session material, which glTF export does not write yet",
            ),
        }
    }
}

impl std::error::Error for GltfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Native(source) => Some(source),
            _ => None,
        }
    }
}

/// Encodes whole 3-byte groups only: every buffer length here is a multiple of 12, so no `=`
/// padding exists.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    debug_assert_eq!(bytes.len() % 3, 0);
    let mut out = String::with_capacity(bytes.len() / 3 * 4);
    for chunk in bytes.as_chunks::<3>().0 {
        let word = u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8 | u32::from(chunk[2]);
        for shift in [18, 12, 6, 0] {
            out.push(char::from(ALPHABET[(word >> shift & 63) as usize]));
        }
    }
    out
}

/// Splits a quad along the diagonal that stays inside it. The fan from corner 0 is right unless
/// the quad is concave at corner 1 or 3, where its two triangles face opposite ways; then the
/// 1-3 diagonal is used. Non-planar quads stay PROVISIONAL against Maya's own split.
fn quad_split(corners: &[usize], positions: &[[f32; 3]]) -> [[usize; 3]; 2] {
    let at = |i: usize| DVec3::from_array(positions[corners[i]].map(f64::from));
    let normal = |a: usize, b: usize, c: usize| (at(b) - at(a)).cross(at(c) - at(a));
    let fan = [
        [corners[0], corners[1], corners[2]],
        [corners[0], corners[2], corners[3]],
    ];
    if normal(0, 1, 2).dot(normal(0, 2, 3)) < 0.0 {
        [
            [corners[0], corners[1], corners[3]],
            [corners[1], corners[2], corners[3]],
        ]
    } else {
        fan
    }
}

struct Mesh {
    name: String,
    positions: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
}

fn collect(
    scene: &NativeScene,
    instances: &[&ViewInstance],
) -> Result<(Vec<Mesh>, usize), GltfError> {
    let mut meshes = Vec::new();
    let mut bytes = 0usize;
    for instance in instances {
        if scene.uv_set(instance.shape()).is_some() {
            return Err(GltfError::SessionUvs);
        }
        let shape = scene.shape(instance.shape()).map_err(GltfError::Native)?;
        if scene.has_assigned_session_material(shape) {
            return Err(GltfError::SessionMaterial);
        }
        let name = scene
            .node(instance.transform())
            .map_err(GltfError::Native)?
            .name()
            .to_owned();
        let mesh = shape.mesh();
        let world = instance.world_matrix();
        let mut positions = Vec::new();
        positions
            .try_reserve_exact(mesh.point_count())
            .map_err(|_| GltfError::TooLarge)?;
        for index in 0..mesh.point_count() {
            let local =
                DVec3::from_array(mesh.effective_point_cm(index).map_err(GltfError::Native)?);
            let meters = world.transform_point3(local) / 100.0;
            let point = [meters.x as f32, meters.y as f32, meters.z as f32];
            // View admission already bounds positions in f32 centimeters; this stays as a guard.
            if !point.iter().all(|c| c.is_finite()) {
                return Err(GltfError::NonFinite);
            }
            positions.push(point);
        }
        let mirrored = world.determinant() < 0.0;
        let mut triangles = Vec::new();
        for face in instance.faces() {
            let corners = face.corners();
            let split: Vec<[usize; 3]> = if corners.len() == 4 {
                quad_split(corners, &positions).to_vec()
            } else {
                face.triangles().collect()
            };
            for [a, b, c] in split {
                let index = |corner: usize| u32::try_from(corner).map_err(|_| GltfError::TooLarge);
                let (a, b, c) = (index(a)?, index(b)?, index(c)?);
                triangles.try_reserve(1).map_err(|_| GltfError::TooLarge)?;
                triangles.push(if mirrored { [a, c, b] } else { [a, b, c] });
            }
        }
        if triangles.is_empty() || positions.is_empty() {
            // glTF requires accessor counts of at least 1.
            continue;
        }
        bytes = positions
            .len()
            .checked_mul(12)
            .and_then(|p| triangles.len().checked_mul(12)?.checked_add(p))
            .and_then(|m| bytes.checked_add(m))
            .filter(|total| *total <= MAX_BUFFER_BYTES)
            .ok_or(GltfError::TooLarge)?;
        meshes.push(Mesh {
            name,
            positions,
            triangles,
        });
    }
    Ok((meshes, bytes))
}

/// Builds the glTF JSON and the binary buffer. The buffer holds, per mesh, its POSITION floats
/// then its u32 indices, each section 4-byte aligned because every element is 4 bytes.
fn build(meshes: &[Mesh], bytes: usize) -> (Value, Vec<u8>) {
    let mut buffer = Vec::with_capacity(bytes);
    let (mut views, mut accessors, mut gltf_meshes, mut nodes) = (vec![], vec![], vec![], vec![]);
    for mesh in meshes {
        let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
        for point in &mesh.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        let position_offset = buffer.len();
        for point in &mesh.positions {
            for value in point {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
        }
        let index_offset = buffer.len();
        for triangle in &mesh.triangles {
            for index in triangle {
                buffer.extend_from_slice(&index.to_le_bytes());
            }
        }
        let first = views.len();
        views.push(json!({"buffer":0,"byteOffset":position_offset,
            "byteLength":mesh.positions.len() * 12,"target":ARRAY_BUFFER}));
        views.push(json!({"buffer":0,"byteOffset":index_offset,
            "byteLength":mesh.triangles.len() * 12,"target":ELEMENT_ARRAY_BUFFER}));
        let accessor = accessors.len();
        accessors.push(json!({"bufferView":first,"componentType":FLOAT,
            "count":mesh.positions.len(),"type":"VEC3","min":min,"max":max}));
        accessors.push(json!({"bufferView":first + 1,"componentType":UNSIGNED_INT,
            "count":mesh.triangles.len() * 3,"type":"SCALAR"}));
        nodes.push(json!({"name":mesh.name,"mesh":gltf_meshes.len()}));
        gltf_meshes.push(json!({"name":mesh.name,"primitives":[
            {"attributes":{"POSITION":accessor},"indices":accessor + 1,"mode":4}]}));
    }
    let gltf_buffer = json!({"byteLength":buffer.len()});
    let document = json!({
        "asset":{"version":"2.0","generator":"AztecCraft",
            "extras":{"units":"meters","sourceUnits":"centimeters","upAxis":"+Y"}},
        "scene":0,
        "scenes":[{"nodes":(0..nodes.len()).collect::<Vec<_>>()}],
        "nodes":nodes,"meshes":gltf_meshes,"accessors":accessors,
        "bufferViews":views,"buffers":[gltf_buffer],
    });
    (document, buffer)
}

fn encode(meshes: &[Mesh], bytes: usize, format: GltfFormat) -> Result<Vec<u8>, GltfError> {
    match format {
        GltfFormat::Gltf => {
            let (mut document, buffer) = build(meshes, bytes);
            document["buffers"][0]["uri"] = Value::String(format!(
                "data:application/octet-stream;base64,{}",
                base64(&buffer)
            ));
            serde_json::to_vec(&document).map_err(|_| GltfError::TooLarge)
        }
        GltfFormat::Glb => {
            let (document, bin) = build(meshes, bytes);
            let mut json = serde_json::to_vec(&document).map_err(|_| GltfError::TooLarge)?;
            json.resize(json.len().next_multiple_of(4), b' ');
            let total = 12usize
                .checked_add(8 + json.len())
                .and_then(|n| n.checked_add(8 + bin.len()))
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(GltfError::TooLarge)?;
            let mut out = Vec::with_capacity(total as usize);
            out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
            out.extend_from_slice(&2u32.to_le_bytes());
            out.extend_from_slice(&total.to_le_bytes());
            for (kind, data) in [(CHUNK_JSON, &json), (CHUNK_BIN, &bin)] {
                out.extend_from_slice(&(data.len() as u32).to_le_bytes());
                out.extend_from_slice(&kind.to_le_bytes());
                out.extend_from_slice(data);
            }
            Ok(out)
        }
    }
}

/// Writes the given prepared mesh instances to a new `.gltf` or `.glb` file (chosen by the path's
/// extension) in world space and meters. The destination must not exist; nothing is written on
/// refusal. `may_install` is asked before the staged file is created and again before it is given
/// the destination's name; a refusal returns [`GltfError::Cancelled`] and leaves nothing behind.
pub fn export_native_new(
    path: &Path,
    scene: &NativeScene,
    instances: &[&ViewInstance],
    may_install: &dyn Fn() -> bool,
) -> Result<GltfSummary, GltfError> {
    let format = GltfFormat::from_path(path).ok_or(GltfError::UnknownExtension)?;
    let (meshes, bytes) = collect(scene, instances)?;
    let summary = GltfSummary {
        objects: meshes.len(),
        vertices: meshes.iter().map(|m| m.positions.len()).sum(),
        triangles: meshes.iter().map(|m| m.triangles.len()).sum(),
    };
    if summary.triangles == 0 {
        return Err(GltfError::NothingToExport);
    }
    let contents = encode(&meshes, bytes, format)?;
    write_new_staged(path, &contents, may_install).map_err(|error| match error {
        StagedWriteError::Cancelled => GltfError::Cancelled,
        StagedWriteError::Io { path, source } => GltfError::Io { path, source },
    })?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::quad_split;

    #[test]
    fn concave_quad_splits_along_the_inner_diagonal() {
        // Arrowhead with the reflex corner at index 1: the 0-2 diagonal leaves the quad.
        let p = [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [4.0, 0.0, 0.0],
            [1.0, 4.0, 0.0],
        ];
        assert_eq!(quad_split(&[0, 1, 2, 3], &p), [[0, 1, 3], [1, 2, 3]]);
        let square = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        assert_eq!(quad_split(&[0, 1, 2, 3], &square), [[0, 1, 2], [0, 2, 3]]);
    }
}
