use super::{add, material::material_name, mul, resource};
use crate::maya_ascii::{AsciiError, MAX_BYTES, Work};
use crate::native_scene::{
    DirectedEdge, NativeError, NativeMesh, NativeMeshShape, NativeScene, NativeTransformNode,
    Points, UvSet,
    admission::{self, Budget},
    authored::{self, AuthoredIneligible, UvPolicy, names},
    is_valid_name,
    session_material::{Lambert, MAX_SESSION_MATERIALS, ShadingGroup},
};
use std::{collections::HashSet, error::Error, fmt};

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
const UVSN_STATEMENT: &str = "\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n";
const CUVS_STATEMENT: &str = "\tsetAttr \".cuvs\" -type \"string\" \"map1\";\n";
const UV_HEADER_BYTES: usize =
    ARRAY_HEADER_BYTES + ".uvst[0].uvsp".len() - ".vt".len() + " -type \"float2\"".len();
const UV_SCALAR_BYTES: usize = 1 + 2 + 44 + 9;
const UV_ROW_BYTES: usize = 2 * UV_SCALAR_BYTES + "\t \n".len();
const MU_FACE_BYTES: usize = " mu 0 ".len() + "2147483647".len();
const MU_ID_BYTES: usize = " 2147483647".len();
const LINK_OPENER: &str = "connectAttr \"";
const LINK_MID: &str = ".iog\" \"";
const LINK_END: &str = ".dsm\" -na;\n";
const DEFAULT_GROUP: &str = ":initialShadingGroup";
const LINK_SYNTAX_BYTES: usize =
    LINK_OPENER.len() + LINK_MID.len() + DEFAULT_GROUP.len() + LINK_END.len();
const LAMBERT_OPENER: &str = "createNode lambert -n \"";
const COLOR_OPENER: &str = "\tsetAttr \".c\" -type \"float3\" ";
const GROUP_OPENER: &str = "createNode shadingEngine -n \"";
const GROUP_RO: &str = "\tsetAttr \".ro\" yes;\n";
const OC_OPENER: &str = "connectAttr \"";
const OC_MID: &str = ".oc\" \"";
const SS_END: &str = ".ss\";\n";
const PA_MID: &str = ".pa\" \":renderPartition.st\" -na;\n";
const MSG_MID: &str = ".msg\" \":defaultShaderList1.s\" -na;\n";
/// Fixed syntax and colour per material; its two names (three uses each) are counted apart.
const MATERIAL_SYNTAX_BYTES: usize = LAMBERT_OPENER.len()
    + GROUP_OPENER.len()
    + 2 * CLOSER.len()
    + COLOR_OPENER.len()
    + 3 * UV_SCALAR_BYTES
    + GROUP_RO.len()
    + 2 * OC_OPENER.len()
    + OC_MID.len()
    + SS_END.len()
    + OC_OPENER.len()
    + PA_MID.len()
    + MSG_MID.len();
const UV_STATEMENT_BYTES: usize = UVSN_STATEMENT.len() + CUVS_STATEMENT.len();

/// Whether each mesh shape is written into the default shading group with a trailing
/// `connectAttr "<mesh>.iog" ":initialShadingGroup.dsm" -na;`. `instObjGroups` into
/// `dagSetMembers` puts the whole object in the set (Maya 2027 Nodes/objectSet and
/// Nodes/shadingEngine); the `-na` and the position after all nodes are PROVISIONAL (#5). Save
/// always emits; the reopen gate re-emits whichever layout the file declares by its link count,
/// so files saved before links existed (`Omit`) still reopen (#156).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefaultLinks {
    Omit,
    Emit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsciiWriteRefusal {
    SourcedScene,
    Curves,
    Joints,
    SessionMaterials,
    Keys,
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
    SessionUvs,
    IndexOutOfRange,
    ResourceLimit,
    TooLarge,
}
impl fmt::Display for AsciiWriteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourcedScene => "retained Maya sources cannot use the authored prefix",
            Self::Curves => "curve declarations are unsupported",
            Self::Joints => "joints are session-only and cannot be saved yet",
            Self::SessionMaterials => {
                "the session material table is not one the authored layout can carry"
            }
            Self::Keys => "session keys are not saved",
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
            Self::SessionUvs => "session UVs are not saved; use Delete UVs or Undo to save",
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
fn mesh_uv_work(faces: usize, corners: usize) -> Result<usize, AsciiError> {
    mul(
        2,
        add(
            add(UV_HEADER_BYTES, mul(corners, UV_ROW_BYTES + MU_ID_BYTES)?)?,
            add(
                mul(faces, MU_FACE_BYTES)?,
                ARRAY_END.len() + UV_STATEMENT_BYTES,
            )?,
        )?,
    )
}
/// The material block, charged per material and bounded by `MAX_SESSION_MATERIALS`; a group
/// name reappears in the link of each shape assigned to it.
fn material_work(scene: &NativeScene, work: &mut Work) -> Result<(), AsciiError> {
    let table = &scene.session_materials;
    if table.materials.len() > MAX_SESSION_MATERIALS || table.groups.len() > MAX_SESSION_MATERIALS {
        return Err(resource());
    }
    work.charge(mul(
        table.materials.len().saturating_sub(1),
        MATERIAL_SYNTAX_BYTES,
    )?)?;
    for name in table
        .materials
        .iter()
        .skip(1)
        .map(Lambert::name)
        .chain(table.groups.iter().skip(1).map(ShadingGroup::name))
    {
        work.charge(mul(3, name.len())?)?;
    }
    for group in table.assignments.values() {
        if let Some(group) = table.groups.get(*group) {
            work.charge(group.name().len())?;
        }
    }
    Ok(())
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
    work.charge(mul(s, LINK_SYNTAX_BYTES)?)?;
    material_work(scene, &mut work)?;
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
        if let Some(set) = scene.uv_set(shape.id()) {
            work.charge(mul(32, set.corner_count())?)?;
            work.charge(mesh_uv_work(set.faces().len(), set.corner_count())?)?;
        }
    }
    Ok(())
}

fn check(scene: &NativeScene) -> Result<(), AsciiWriteRefusal> {
    authored::check_content_kind(scene, UvPolicy::Admit)?;
    resource_admission(scene).map_err(|_| AsciiWriteRefusal::ResourceLimit)?;
    authored::check_scene(scene)?;
    check_materials(scene)?;
    // Every field is decided here, so a new `NativeScene` field breaks this build until it is.
    let NativeScene {
        // Covered by `check_content_kind` and `check_scene` above.
        nodes: _,
        shapes: _,
        curves: _,
        dag: _,
        units: _,
        source: _,
        // Fresh per scene; the file stores no document identity.
        authored_document_id: _,
        next_id: _,
        next_native_id: _,
        // Diffuse edits bound into retained source bytes: set only with a source, which
        // `check_content_kind` refuses.
        materials: _,
        // Ids of deleted source nodes: only in a sourced scene, which is refused above.
        removed: _,
        // Created, assigned or reassigned materials: `check_materials` above.
        session_materials: _,
        // Emitted per shape (`UvPolicy::Admit`); keys and joints are refused by `check_content_kind`.
        session_joints: _,
        session_uvs: _,
        session_keys: _,
    } = scene;
    Ok(())
}

/// The session table must be exactly what the reader rebuilds, so a reopened scene equals it:
/// `lambertN`/`lambertNSG` pairs with distinct N >= 2 that no node or shape name uses, and no
/// stored assignment to the default group (assigning back to it removes the entry).
fn check_materials(scene: &NativeScene) -> Result<(), AsciiWriteRefusal> {
    let table = &scene.session_materials;
    let mut used: HashSet<&str> = names(scene).map(|(_, name)| name).collect();
    for (material, group) in table.materials.iter().zip(&table.groups).skip(1) {
        let (name, group) = (material.name(), group.name());
        if !material_name(name, false)
            || group.strip_suffix("SG") != Some(name)
            || !is_valid_name(name)
        {
            return Err(AsciiWriteRefusal::SessionMaterials);
        }
        if !used.insert(name) || !used.insert(group) {
            return Err(AsciiWriteRefusal::NameCollision);
        }
    }
    if table.assignments.values().any(|group| *group == 0) {
        return Err(AsciiWriteRefusal::SessionMaterials);
    }
    Ok(())
}

impl From<AuthoredIneligible> for AsciiWriteRefusal {
    fn from(ineligible: AuthoredIneligible) -> Self {
        use AuthoredIneligible as I;
        match ineligible {
            I::SourcedScene => Self::SourcedScene,
            I::Curves => Self::Curves,
            I::Joints => Self::Joints,
            I::Keys => Self::Keys,
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
            // The ASCII writer admits UVs, so `check_content_kind` never yields this here.
            I::SessionUvs => Self::SessionUvs,
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
    fn uv_header(&mut self, count: usize) -> Result<(), AsciiWriteRefusal> {
        let count = i32::try_from(count).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
        let last = count
            .checked_sub(1)
            .filter(|last| *last >= 0)
            .ok_or(AsciiWriteRefusal::EmptyMesh)?;
        self.used = 0;
        self.format(format_args!(
            "\tsetAttr -s {count} \".uvst[0].uvsp[0:{last}]\" -type \"float2\"\n"
        ))
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
    uvs: Option<&UvSet>,
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
    let mut next_uv = 0usize;
    for (index, face) in mesh.faces().iter().enumerate() {
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
        if let Some(set) = uvs {
            let uv_corners = set.faces().get(index).map_or(0, Vec::len);
            let count =
                i32::try_from(uv_corners).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
            line.used = 0;
            line.format(format_args!(" mu 0 {count}"))?;
            consume(&line.bytes[..line.used])?;
            for _ in 0..uv_corners {
                let id = i32::try_from(next_uv).map_err(|_| AsciiWriteRefusal::IndexOutOfRange)?;
                next_uv += 1;
                line.used = 0;
                line.format(format_args!(" {id}"))?;
                consume(&line.bytes[..line.used])?;
            }
        }
        consume(b"\n")?;
    }
    consume(ARRAY_END)?;
    let Some(set) = uvs else {
        return Ok(());
    };
    consume(UVSN_STATEMENT.as_bytes())?;
    line.uv_header(set.corner_count())?;
    consume(&line.bytes[..line.used])?;
    for [u, v] in set.faces().iter().flatten() {
        line.used = 0;
        line.format(format_args!("\t{} {}\n", *u as f32, *v as f32))?;
        consume(&line.bytes[..line.used])?;
    }
    consume(ARRAY_END)?;
    consume(CUVS_STATEMENT.as_bytes())
}
/// Each shape's one membership, into its assigned group or the default one.
fn default_links(
    scene: &NativeScene,
    mut consume: impl FnMut(&[u8]) -> Result<(), AsciiWriteRefusal>,
) -> Result<(), AsciiWriteRefusal> {
    let table = &scene.session_materials;
    for node in &scene.nodes {
        let shape = child(scene, node)?;
        let group = match table.assignments.get(&shape.id()) {
            Some(index) => table
                .groups
                .get(*index)
                .ok_or(AsciiWriteRefusal::SessionMaterials)?
                .name(),
            None => DEFAULT_GROUP,
        };
        for bytes in [LINK_OPENER, shape.name(), LINK_MID, group, LINK_END] {
            consume(bytes.as_bytes())?;
        }
    }
    Ok(())
}
/// The session lamberts and shading groups: all nodes, then each pair's three connects.
fn material_nodes(
    scene: &NativeScene,
    mut consume: impl FnMut(&[u8]) -> Result<(), AsciiWriteRefusal>,
) -> Result<(), AsciiWriteRefusal> {
    let table = &scene.session_materials;
    let pairs = || table.materials.iter().zip(&table.groups).skip(1);
    let mut line = ChannelLine {
        bytes: [0; CHANNEL_BYTES],
        used: 0,
    };
    for (material, group) in pairs() {
        for bytes in [LAMBERT_OPENER, material.name(), CLOSER] {
            consume(bytes.as_bytes())?;
        }
        let [r, g, b] = material.color().map(|channel| channel as f32);
        line.used = 0;
        line.format(format_args!("{COLOR_OPENER}{r} {g} {b};\n"))?;
        consume(&line.bytes[..line.used])?;
        for bytes in [GROUP_OPENER, group.name(), CLOSER, GROUP_RO] {
            consume(bytes.as_bytes())?;
        }
    }
    for (material, group) in pairs() {
        let (m, sg) = (material.name(), group.name());
        for bytes in [
            OC_OPENER, m, OC_MID, sg, SS_END, OC_OPENER, sg, PA_MID, OC_OPENER, m, MSG_MID,
        ] {
            consume(bytes.as_bytes())?;
        }
    }
    Ok(())
}
/// Everything after the last node, shared by the byte count and the emitter so they cannot
/// drift. A scene with session materials always links every shape, whatever `links` says.
fn session_block(
    scene: &NativeScene,
    links: DefaultLinks,
    mut consume: impl FnMut(&[u8]) -> Result<(), AsciiWriteRefusal>,
) -> Result<(), AsciiWriteRefusal> {
    let custom = scene.session_materials.materials.len() > 1;
    if custom {
        material_nodes(scene, &mut consume)?;
    }
    if custom || links == DefaultLinks::Emit {
        default_links(scene, consume)?;
    }
    Ok(())
}
fn prefix_bytes(scene: &NativeScene, links: DefaultLinks) -> Result<usize, AsciiWriteRefusal> {
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
        mesh_arrays(shape.mesh(), scene.uv_set(shape.id()), |bytes| {
            length = add_prefix_bytes(length, bytes.len())?;
            Ok(())
        })?;
    }
    session_block(scene, links, |bytes| {
        length = add_prefix_bytes(length, bytes.len())?;
        Ok(())
    })?;
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

/// The one ASCII writer: Save (unix-only), the reopen gate (every platform) and tests call it. Each
/// shape's session UVs are carried (#156); only the binary writer still refuses them.
#[cfg(any(unix, test))]
pub(crate) fn emit_geometry_prefix(scene: &NativeScene) -> Result<Vec<u8>, AsciiWriteRefusal> {
    emit_with_links(scene, DefaultLinks::Emit)
}

pub(crate) fn emit_with_links(
    scene: &NativeScene,
    links: DefaultLinks,
) -> Result<Vec<u8>, AsciiWriteRefusal> {
    check(scene)?;
    let length = prefix_bytes(scene, links)?;
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
        mesh_arrays(shape.mesh(), scene.uv_set(shape.id()), |bytes| {
            append(&mut out, length, bytes)
        })?;
    }
    session_block(scene, links, |bytes| append(&mut out, length, bytes))?;
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
