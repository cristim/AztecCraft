use crate::native_scene::{NativeError, NativeScene};
use crate::scene::{Scene, SceneError};
use crate::view_scene::ViewInstance;
use glam::DVec3;
use std::{
    fmt,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

fn refuse_occupied(path: &Path) -> Result<(), ObjError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(ObjError::Io {
            path: path.into(),
            source: io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ObjError::Io {
            path: path.into(),
            source,
        }),
    }
}

fn write_name(writer: &mut impl Write, name: &str) -> io::Result<()> {
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            writer.write_all(&[byte])?;
        } else {
            write!(writer, "%{byte:02X}")?;
        }
    }
    Ok(())
}

/// Counts written by [`export_native_new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportSummary {
    pub objects: usize,
    pub vertices: usize,
    pub faces: usize,
}

/// Writes the given prepared mesh instances to a new OBJ file in world space, one `o` record per
/// instance named after its transform. Positions are recomputed in f64 from the authoring scene
/// (centimeters, Maya's internal unit) rather than taken from the f32 display vertices. A
/// mirrored instance reverses its faces so they keep facing outward. Normals, UVs, groups and
/// materials are not written. The destination must not exist; nothing is written on refusal.
pub fn export_native_new(
    path: &Path,
    scene: &NativeScene,
    instances: &[&ViewInstance],
) -> Result<ExportSummary, ObjError> {
    if instances.is_empty() {
        return Err(ObjError::NothingToExport);
    }
    let mut objects = Vec::new();
    objects
        .try_reserve_exact(instances.len())
        .map_err(|_| ObjError::TooLarge)?;
    let mut summary = ExportSummary {
        objects: 0,
        vertices: 0,
        faces: 0,
    };
    for instance in instances {
        let name = scene
            .node(instance.transform())
            .map_err(ObjError::Native)?
            .name();
        let mesh = scene
            .shape(instance.shape())
            .map_err(ObjError::Native)?
            .mesh();
        let world = instance.world_matrix();
        let count = mesh.point_count();
        let mut points = Vec::new();
        points
            .try_reserve_exact(count)
            .map_err(|_| ObjError::TooLarge)?;
        for index in 0..count {
            let local =
                DVec3::from_array(mesh.effective_point_cm(index).map_err(ObjError::Native)?);
            let point = world.transform_point3(local);
            if !point.is_finite() {
                return Err(ObjError::NonFinite);
            }
            points.push(point);
        }
        let mirrored = world.determinant() < 0.0;
        summary.objects += 1;
        summary.vertices = summary
            .vertices
            .checked_add(count)
            .ok_or(ObjError::TooLarge)?;
        summary.faces = summary
            .faces
            .checked_add(instance.faces().len())
            .ok_or(ObjError::TooLarge)?;
        objects.push((name, points, *instance, mirrored));
    }
    refuse_occupied(path)?;
    craft_atomic::write_new_with(path, |file| {
        let mut writer = BufWriter::new(file);
        writeln!(
            writer,
            "# AztecCraft world-space polygon geometry (centimeters)"
        )?;
        let mut offset = 1usize;
        for (name, points, instance, mirrored) in &objects {
            write!(writer, "o ")?;
            write_name(&mut writer, name)?;
            writeln!(writer)?;
            for point in points {
                writeln!(writer, "v {} {} {}", point.x, point.y, point.z)?;
            }
            for face in instance.faces() {
                write!(writer, "f")?;
                let corners = face.corners();
                let mut write_corner = |corner: usize| match corner.checked_add(offset) {
                    Some(index) => write!(writer, " {index}"),
                    None => Err(io::Error::from(io::ErrorKind::InvalidData)),
                };
                if *mirrored {
                    corners.iter().rev().try_for_each(|&c| write_corner(c))?;
                } else {
                    corners.iter().try_for_each(|&c| write_corner(c))?;
                }
                writeln!(writer)?;
            }
            offset = offset
                .checked_add(points.len())
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        }
        writer.flush()
    })
    .map_err(|source| ObjError::Io {
        path: path.into(),
        source,
    })?;
    Ok(summary)
}

// Names use UTF-8 percent encoding except ASCII letters, digits, '-', '_' and '.'.
// Encoding '%' too preserves distinctions while preventing whitespace/comment/directive injection.
pub fn export_new(path: &Path, scene: &Scene) -> Result<(), ObjError> {
    scene.validate().map_err(ObjError::Scene)?;
    refuse_occupied(path)?;
    craft_atomic::write_new_with(path, |file| {
        let mut writer = BufWriter::new(file);
        writeln!(writer, "# AztecCraft world-space polygon geometry")?;
        let mut offset = 1;
        for node in scene.nodes() {
            write!(writer, "o ")?;
            write_name(&mut writer, node.name())?;
            writeln!(writer)?;
            for vertex in node.world_vertices() {
                writeln!(writer, "v {} {} {}", vertex.x, vertex.y, vertex.z)?;
            }
            let scale = node.transform().scale;
            let reflected = scale.x != 0.0
                && scale.y != 0.0
                && scale.z != 0.0
                && (scale.x.is_sign_negative()
                    ^ scale.y.is_sign_negative()
                    ^ scale.z.is_sign_negative());
            for face in node.mesh().faces() {
                let mut face = *face;
                if reflected {
                    face.reverse();
                }
                writeln!(
                    writer,
                    "f {} {} {} {}",
                    face[0] + offset,
                    face[1] + offset,
                    face[2] + offset,
                    face[3] + offset
                )?;
            }
            offset += node.mesh().vertices().len();
        }
        writer.flush()
    })
    .map_err(|source| ObjError::Io {
        path: path.into(),
        source,
    })
}

#[derive(Debug)]
pub enum ObjError {
    Io { path: PathBuf, source: io::Error },
    Scene(SceneError),
    Native(NativeError),
    NothingToExport,
    NonFinite,
    TooLarge,
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "export OBJ {}: {source}", path.display()),
            Self::Scene(source) => write!(f, "invalid scene for OBJ export: {source}"),
            Self::Native(source) => write!(f, "invalid scene for OBJ export: {source}"),
            Self::NothingToExport => f.write_str("no displayed polygon geometry to export"),
            Self::NonFinite => f.write_str("a world-space position is not finite"),
            Self::TooLarge => f.write_str("the export exceeds addressable memory"),
        }
    }
}

impl std::error::Error for ObjError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Scene(source) => Some(source),
            Self::Native(source) => Some(source),
            Self::NothingToExport | Self::NonFinite | Self::TooLarge => None,
        }
    }
}
