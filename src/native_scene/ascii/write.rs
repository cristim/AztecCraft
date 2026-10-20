use super::{add, mul, resource};
use crate::maya_ascii::{AsciiError, MAX_BYTES, Work};
use crate::native_scene::{
    DirectedEdge, NativeError, NativeMesh, NativeMeshShape, NativeScene, NativeTransformNode,
    Points,
    admission::{self, Budget},
    authored::{self, AuthoredIneligible, names},
};
use std::{error::Error, fmt};

const HEADER: &str = "//Maya ASCII 2026 scene\nrequires maya \"2026\";\ncurrentUnit -l centimeter -a degree -t film;\nfileInfo \"AztecCraft\" \"1\";\n";
const OPENER: &str = "createNode transform -n \"";
const CLOSER: &str = "\";\n";
const UUID_OPENER: &str = "\trename -uid \"";
const UUID_BYTES: usize = uuid::fmt::Hyphenated::LENGTH;
const UUID_STATEMENT_BYTES: usize = UUID_OPENER.len() + UUID_BYTES + CLOSER.len();
const MESH_OPENER: &str = "createNode mesh -n \"";
const PARENT: &str = "\" -p \"";
const MESH_SYNTAX_BYTES: usize = MESH_OPENER.len() + PARENT.len() + CLOSER.len();
const SCALAR_BYTES: usize = 1 + 2 + 323 + 17;
const CHANNEL_BYTES: usize = 3 * SCALAR_BYTES + "\tsetAttr \".t\" -type \"double3\"   ;\n".len();
const FORMATTING_WORK: usize = 2 * (3 * CHANNEL_BYTES + UUID_BYTES);
const POINT_ROW_BYTES: usize = 3 * (1 + 2 + 44 + 9) + "\t  \n".len();
const EDGE_ROW_BYTES: usize = 2 * "2147483647".len() + "\t  0\n".len();
const ARRAY_HEADER_BYTES: usize = 2 * "2147483647".len() + "\tsetAttr -s  \".vt[0:]\"\n".len();
const FACE_HEADER_BYTES: usize = ARRAY_HEADER_BYTES + " -type \"polyFaces\"".len();
const FACE_ROW_BYTES: usize = "\tf \n".len() + "2147483647".len() + 1;
const FACE_TOKEN_BYTES: usize = " -2147483648".len();
const ARRAY_END: &[u8] = b";\n";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AsciiWriteRefusal {
    SourcedScene,
    Curves,
    InvalidScene(NativeError),
    UnsupportedUnits,
    NonAuthoredHierarchy,
    InvalidName,
    ReservedName,
    NameCollision,
    EmptyMesh,
    NonDefaultPivot,
    TransformExtras,
    TransformRestriction,
    UnsupportedTransformEffect,
    TransformVisibility,
    ShapeRestriction,
    ShapeVisibility,
    Double3Points,
    PointOffsets,
    EdgeSmoothing,
    FaceHoles,
    UnsupportedMeshChannels,
    IndexOutOfRange,
    ResourceLimit,
    TooLarge,
}
impl fmt::Display for AsciiWriteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourcedScene => "retained Maya sources cannot use the authored prefix",
            Self::Curves => "curve declarations are unsupported",
            Self::InvalidScene(error) => return error.fmt(f),
            Self::UnsupportedUnits => "only centimeter, degree and film units are supported",
            Self::NonAuthoredHierarchy => "only flat transform-over-mesh ownership is supported",
            Self::InvalidName => "node names require the supported ASCII identifier subset",
            Self::ReservedName => "node name collides with a default Maya node",
            Self::NameCollision => "two declared nodes share a name",
            Self::EmptyMesh => "a mesh requires points and faces",
            Self::NonDefaultPivot => "nondefault pivots are unsupported",
            Self::TransformExtras => "explicit transform extras are unsupported",
            Self::TransformRestriction => "transform restrictions are unsupported",
            Self::UnsupportedTransformEffect => "transform effects are unsupported",
            Self::TransformVisibility => "transform requires proved static visible geometry",
            Self::ShapeRestriction => "shape restrictions are unsupported",
            Self::ShapeVisibility => "shape requires proved static visible geometry",
            Self::Double3Points => "double-precision mesh points are unsupported",
            Self::PointOffsets => "mesh point offsets are unsupported",
            Self::EdgeSmoothing => "mesh edges require known hard smoothing",
            Self::FaceHoles => "mesh face holes are unsupported",
            Self::UnsupportedMeshChannels => "additional mesh channels are unsupported",
            Self::IndexOutOfRange => "mesh array count or endpoint exceeds signed 32-bit range",
            Self::ResourceLimit => "authored prefix resource policy exceeded",
            Self::TooLarge => "authored prefix exceeds the 16 MiB byte limit",
        })
    }
}
impl Error for AsciiWriteRefusal {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidScene(error) => Some(error),
            _ => None,
        }
    }
}

fn charge_face_counts(
    work: &mut Work,
    outer_references: usize,
    hole_containers: usize,
) -> Result<(), AsciiError> {
    work.charge(mul(32, add(outer_references, hole_containers)?)?)
}
fn mesh_declaration_work(nodes: usize, shapes: usize, dag: usize) -> Result<usize, AsciiError> {
    add(
        mul(mul(2, nodes)?, add(dag, shapes)?)?,
        mul(mul(2, nodes)?, MESH_SYNTAX_BYTES + UUID_STATEMENT_BYTES)?,
    )
}
fn mesh_array_work(points: usize, edges: usize) -> Result<usize, AsciiError> {
    mul(
        2,
        add(
            add(CHANNEL_BYTES, mul(points, POINT_ROW_BYTES)?)?,
            add(
                mul(edges, EDGE_ROW_BYTES)?,
                2 * ARRAY_HEADER_BYTES + 2 * ARRAY_END.len(),
            )?,
        )?,
    )
}
fn mesh_face_work(faces: usize, corners: usize) -> Result<usize, AsciiError> {
    mul(
        2,
        add(
            add(FACE_HEADER_BYTES, mul(faces, FACE_ROW_BYTES)?)?,
            add(mul(corners, FACE_TOKEN_BYTES)?, ARRAY_END.len())?,
        )?,
    )
}
fn resource_admission(scene: &NativeScene) -> Result<(), AsciiError> {
    let n = scene.nodes.len();
    let s = scene.shapes.len();
    let d = scene.dag.len();
    let t = add(n, s)?;
    Budget::new(admission::validation_bytes(n, t, d).map_err(|_| resource())?)
        .map_err(|_| resource())?;
    let mut work = Work::new();
    work.charge(mul(n, FORMATTING_WORK)?)?;
    work.charge(mesh_declaration_work(n, s, d)?)?;
    work.charge(mul(mul(2, t)?, d)?)?;
    work.charge(mul(n, add(d, s)?)?)?;
    for count in [
        n,
        s,
        d,
        scene.session_materials.assignments.len(),
        scene.session_materials.materials.len(),
        scene.session_materials.groups.len(),
        1,
    ] {
        work.charge(mul(32, count)?)?;
    }
    for (_, name) in names(scene) {
        work.charge(mul(t, name.len())?)?;
        work.charge(mul(2, name.len())?)?;
    }
    for shape in &scene.shapes {
        let mesh = shape.mesh();
        work.charge(mesh_array_work(mesh.point_count(), mesh.edges().len())?)?;
        let mut corners = 0;
        for count in [mesh.point_count(), mesh.edges().len(), mesh.faces().len()] {
            work.charge(mul(32, count)?)?;
        }
        for face in mesh.faces() {
            corners = add(corners, face.outer().len())?;
            charge_face_counts(&mut work, face.outer().len(), face.holes().len())?;
            for hole in face.holes() {
                work.charge(mul(32, hole.len())?)?;
            }
        }
        work.charge(mesh_face_work(mesh.faces().len(), corners)?)?;
    }
    Ok(())
}

fn check(scene: &NativeScene) -> Result<(), AsciiWriteRefusal> {
    authored::check_content_kind(scene)?;
    resource_admission(scene).map_err(|_| AsciiWriteRefusal::ResourceLimit)?;
    authored::check_scene(scene)?;
    Ok(())
}

impl From<AuthoredIneligible> for AsciiWriteRefusal {
    fn from(ineligible: AuthoredIneligible) -> Self {
        use AuthoredIneligible as I;
        match ineligible {
            I::SourcedScene => Self::SourcedScene,
            I::Curves => Self::Curves,
            I::InvalidScene(error) => Self::InvalidScene(error),
            I::UnsupportedUnits => Self::UnsupportedUnits,
            I::NonAuthoredHierarchy => Self::NonAuthoredHierarchy,
            I::InvalidName => Self::InvalidName,
            I::ReservedName => Self::ReservedName,
            I::NameCollision => Self::NameCollision,
            I::EmptyMesh => Self::EmptyMesh,
            I::NonDefaultPivot => Self::NonDefaultPivot,
            I::TransformExtras => Self::TransformExtras,
            I::TransformRestriction => Self::TransformRestriction,
            I::UnsupportedTransformEffect => Self::UnsupportedTransformEffect,
            I::TransformVisibility => Self::TransformVisibility,
            I::ShapeRestriction => Self::ShapeRestriction,
            I::ShapeVisibility => Self::ShapeVisibility,
            I::Double3Points => Self::Double3Points,
            I::PointOffsets => Self::PointOffsets,
            I::EdgeSmoothing => Self::EdgeSmoothing,
            I::FaceHoles => Self::FaceHoles,
            I::UnsupportedMeshChannels => Self::UnsupportedMeshChannels,
        }
    }
}

fn child<'a>(
    scene: &'a NativeScene,
    node: &NativeTransformNode,
) -> Result<&'a NativeMeshShape, AsciiWriteRefusal> {
    Ok(authored::shape_of(scene, node)?)
}

fn declaration_bytes(nodes: usize, name_bytes: usize) -> Result<usize, AsciiWriteRefusal> {
    nodes
        .checked_mul(OPENER.len() + CLOSER.len())
        .and_then(|fixed| fixed.checked_add(name_bytes))
        .and_then(|body| body.checked_add(HEADER.len()))
        .filter(|length| *length <= MAX_BYTES)
        .ok_or(AsciiWriteRefusal::TooLarge)
}

struct ChannelLine {
    bytes: [u8; CHANNEL_BYTES],
    used: usize,
}
impl ChannelLine {
    fn format(&mut self, arguments: fmt::Arguments<'_>) -> Result<(), AsciiWriteRefusal> {
        fmt::write(self, arguments).map_err(|_| AsciiWriteRefusal::ResourceLimit)
    }
    fn array_header(&mut self, attribute: &str, count: usize) -> Result<(), AsciiWriteRefusal> {
        let count = i32::try_from(count).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
        let last = count
            .checked_sub(1)
            .filter(|last| *last >= 0)
            .ok_or(AsciiWriteRefusal::EmptyMesh)?;
        self.used = 0;
        self.format(format_args!(
            "\tsetAttr -s {count} \"{attribute}[0:{last}]\"\n"
        ))
    }
    fn edge_row(&mut self, endpoints: [usize; 2]) -> Result<(), AsciiWriteRefusal> {
        let a = i32::try_from(endpoints[0]).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
        let b = i32::try_from(endpoints[1]).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
        self.used = 0;
        self.format(format_args!("\t{a} {b} 0\n"))
    }
    fn face_header(&mut self, count: usize) -> Result<(), AsciiWriteRefusal> {
        let count = i32::try_from(count).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
        let last = count
            .checked_sub(1)
            .filter(|last| *last >= 0)
            .ok_or(AsciiWriteRefusal::EmptyMesh)?;
        self.used = 0;
        self.format(format_args!(
            "\tsetAttr -s {count} \".fc[0:{last}]\" -type \"polyFaces\"\n"
        ))
    }
}
fn signed_face_edge(edge: DirectedEdge) -> Result<i32, AsciiWriteRefusal> {
    let id = i64::try_from(edge.edge).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
    let token = if edge.reversed { -id - 1 } else { id };
    i32::try_from(token).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)
}
impl fmt::Write for ChannelLine {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self
            .used
            .checked_add(text.len())
            .filter(|end| *end <= self.bytes.len())
            .ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.used..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.used = end;
        Ok(())
    }
}
fn channel_lines(
    node: &NativeTransformNode,
) -> impl Iterator<Item = Result<ChannelLine, AsciiWriteRefusal>> {
    let transform = node.transform();
    [
        (".t", transform.translation, [0.0; 3]),
        (".r", transform.rotation, [0.0; 3]),
        (".s", transform.scale, [1.0; 3]),
    ]
    .into_iter()
    .filter(|(_, values, default)| values.map(f64::to_bits) != default.map(f64::to_bits))
    .map(|(attribute, values, _)| {
        let mut line = ChannelLine {
            bytes: [0; CHANNEL_BYTES],
            used: 0,
        };
        line.format(format_args!(
            "\tsetAttr \"{attribute}\" -type \"double3\" {} {} {};\n",
            values[0], values[1], values[2]
        ))?;
        Ok(line)
    })
}
fn add_prefix_bytes(base: usize, added: usize) -> Result<usize, AsciiWriteRefusal> {
    base.checked_add(added)
        .filter(|length| *length <= MAX_BYTES)
        .ok_or(AsciiWriteRefusal::TooLarge)
}
fn mesh_arrays(
    mesh: &NativeMesh,
    mut consume: impl FnMut(&[u8]) -> Result<(), AsciiWriteRefusal>,
) -> Result<(), AsciiWriteRefusal> {
    let Points::Float3(points) = mesh.points() else {
        return Err(AsciiWriteRefusal::Double3Points);
    };
    let mut line = ChannelLine {
        bytes: [0; CHANNEL_BYTES],
        used: 0,
    };
    line.array_header(".vt", points.len())?;
    consume(&line.bytes[..line.used])?;
    for point in points {
        line.used = 0;
        line.format(format_args!("\t{} {} {}\n", point[0], point[1], point[2]))?;
        consume(&line.bytes[..line.used])?;
    }
    consume(ARRAY_END)?;
    line.array_header(".ed", mesh.edges().len())?;
    consume(&line.bytes[..line.used])?;
    for edge in mesh.edges() {
        line.edge_row(edge.endpoints())?;
        consume(&line.bytes[..line.used])?;
    }
    consume(ARRAY_END)?;
    line.face_header(mesh.faces().len())?;
    consume(&line.bytes[..line.used])?;
    for face in mesh.faces() {
        let corners =
            i32::try_from(face.outer().len()).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
        line.used = 0;
        line.format(format_args!("\tf {corners}"))?;
        consume(&line.bytes[..line.used])?;
        for edge in face.outer() {
            let token = signed_face_edge(*edge)?;
            line.used = 0;
            line.format(format_args!(" {token}"))?;
            consume(&line.bytes[..line.used])?;
        }
        consume(b"\n")?;
    }
    consume(ARRAY_END)
}
fn prefix_bytes(scene: &NativeScene) -> Result<usize, AsciiWriteRefusal> {
    let name_bytes = scene
        .nodes
        .iter()
        .try_fold(0, |sum, n| add(sum, n.name().len()))
        .map_err(|_| AsciiWriteRefusal::TooLarge)?;
    let mut length = declaration_bytes(scene.nodes.len(), name_bytes)?;
    for node in &scene.nodes {
        if node.transform_uuid().is_some() {
            length = add_prefix_bytes(length, UUID_STATEMENT_BYTES)?;
        }
        for line in channel_lines(node) {
            length = add_prefix_bytes(length, line?.used)?;
        }
        let shape = child(scene, node)?;
        for bytes in [
            MESH_SYNTAX_BYTES,
            shape.name().len(),
            node.name().len(),
            if shape.uuid().is_some() {
                UUID_STATEMENT_BYTES
            } else {
                0
            },
        ] {
            length = add_prefix_bytes(length, bytes)?;
        }
        mesh_arrays(shape.mesh(), |bytes| {
            length = add_prefix_bytes(length, bytes.len())?;
            Ok(())
        })?;
    }
    Ok(length)
}
fn append(out: &mut Vec<u8>, admitted: usize, bytes: &[u8]) -> Result<(), AsciiWriteRefusal> {
    out.len()
        .checked_add(bytes.len())
        .filter(|end| *end <= admitted)
        .ok_or(AsciiWriteRefusal::TooLarge)?;
    out.extend_from_slice(bytes);
    Ok(())
}

#[cfg_attr(not(test), expect(dead_code, reason = "no Editor caller yet"))]
pub(crate) fn emit_geometry_prefix(scene: &NativeScene) -> Result<Vec<u8>, AsciiWriteRefusal> {
    check(scene)?;
    let length = prefix_bytes(scene)?;
    let mut out = Budget::new(0)
        .and_then(|mut b| b.vector::<u8>(length))
        .map_err(|_| AsciiWriteRefusal::ResourceLimit)?;
    append(&mut out, length, HEADER.as_bytes())?;
    for node in &scene.nodes {
        append(&mut out, length, OPENER.as_bytes())?;
        append(&mut out, length, node.name().as_bytes())?;
        append(&mut out, length, CLOSER.as_bytes())?;
        append_uuid(&mut out, length, node.transform_uuid())?;
        for line in channel_lines(node) {
            let line = line?;
            append(&mut out, length, &line.bytes[..line.used])?;
        }
        let shape = child(scene, node)?;
        for bytes in [MESH_OPENER, shape.name(), PARENT, node.name(), CLOSER] {
            append(&mut out, length, bytes.as_bytes())?;
        }
        append_uuid(&mut out, length, shape.uuid())?;
        mesh_arrays(shape.mesh(), |bytes| append(&mut out, length, bytes))?;
    }
    Ok(out)
}

fn append_uuid(
    out: &mut Vec<u8>,
    length: usize,
    uuid: Option<uuid::Uuid>,
) -> Result<(), AsciiWriteRefusal> {
    if let Some(uuid) = uuid {
        let mut buffer = [0; UUID_BYTES];
        append(out, length, UUID_OPENER.as_bytes())?;
        append(
            out,
            length,
            uuid.hyphenated().encode_lower(&mut buffer).as_bytes(),
        )?;
        append(out, length, CLOSER.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
