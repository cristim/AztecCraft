use super::primitive_args::CreateArgs;
use crate::{
    editor_operations::{
        ComponentKind, EdgeSelectionTarget, FaceSelectionTarget, MAX_COMPONENT_INDICES,
        MarqueeMode, Operation, RemoteRequest, SelectRule, SelectionChange,
        SelectionMode as EditorMode, VertexSelectionTarget,
    },
    mcp::{
        args::{
            MutationArgs, decode, mutation, native_id, node_id, schema, scoped, token, unit_point,
        },
        registry::ToolSpec,
    },
};
use azteccraft::{
    native_scene::{EdgeTarget, FaceTarget, NativeId, VertexTarget},
    scene::NodeId,
    selection::ComponentOwner,
    viewport::NdcRect,
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectArgs {
    scene_token: String,
    node_id: Option<String>,
    #[schemars(length(max = 256))]
    instance_path: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SelectionChange")]
enum Change {
    Add,
    Toggle,
    Remove,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ModifySelectionArgs {
    scene_token: String,
    change: Change,
    node_id: String,
    #[schemars(length(max = 256))]
    instance_path: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SelectRule")]
enum Rule {
    All,
    Hierarchy,
    Inverse,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectByRuleArgs {
    scene_token: String,
    rule: Rule,
}

/// A rectangle in normalised device coordinates: x right, y up, both within -1 to 1. The corners
/// may come in either order.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NdcRectArgs {
    // Parsed as f64 so `unit_point` checks the wire value; the schema keeps the f32 format.
    #[schemars(with = "[f32; 2]", inner(range(min = -1.0, max = 1.0)))]
    min: [f64; 2],
    #[schemars(with = "[f32; 2]", inner(range(min = -1.0, max = 1.0)))]
    max: [f64; 2],
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "MarqueeMode")]
enum Marquee {
    Replace,
    Add,
    Remove,
    Toggle,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectInRectArgs {
    scene_token: String,
    rect: NdcRectArgs,
    /// Defaults to replace.
    mode: Option<Marquee>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SelectionMode")]
enum SelectionMode {
    Object,
    Face,
    Hierarchy,
    Vertex,
    Edge,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectionModeArgs {
    scene_token: String,
    mode: SelectionMode,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectFaceArgs {
    scene_token: String,
    target: Option<FaceSelectionTarget>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectVertexArgs {
    scene_token: String,
    target: Option<VertexSelectionTarget>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectEdgeArgs {
    scene_token: String,
    target: Option<EdgeSelectionTarget>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "ComponentKind")]
enum Component {
    Face,
    Vertex,
    Edge,
}

/// The shape instance whose components are selected: the selected object's shape, as
/// `select_face` names it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ComponentOwnerArgs {
    node_id: String,
    shape_id: String,
    #[schemars(length(min = 2, max = 257))]
    instance_path: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ModifyComponentSelectionArgs {
    scene_token: String,
    kind: Component,
    /// Defaults to replace.
    mode: Option<Marquee>,
    owner: ComponentOwnerArgs,
    /// Component indices of the owner's mesh; repeats keep their first position.
    #[schemars(length(max = 262144))]
    indices: Vec<usize>,
}

fn parse_create_primitive(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let parsed = decode::<CreateArgs>(arguments)?.into_parsed()?;
    Ok(scoped(
        token(&parsed.scene_token)?,
        Operation::Create {
            primitive: parsed.primitive,
            options: parsed.options,
            name: parsed.name,
        },
    ))
}

/// Parses an optional `instance_path` of at most 256 decimal NativeIds.
fn parse_instance_path(path: Option<Vec<String>>) -> Result<Option<Vec<NativeId>>, ErrorData> {
    path.map(|path| {
        if path.len() > 256 {
            return Err(ErrorData::invalid_params(
                "instance_path exceeds 256 IDs",
                None,
            ));
        }
        path.into_iter()
            .map(|value| native_id(&value))
            .collect::<Result<Vec<_>, _>>()
    })
    .transpose()
}

fn parse_select_node(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SelectArgs = decode(arguments)?;
    if args.node_id.is_none() && args.instance_path.is_some() {
        return Err(ErrorData::invalid_params(
            "Clearing selection cannot include an instance_path",
            None,
        ));
    }
    let instance_path = parse_instance_path(args.instance_path)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Select {
            node: args.node_id.as_deref().map(node_id).transpose()?,
            instance_path,
        },
    ))
}

fn parse_modify_selection(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ModifySelectionArgs = decode(arguments)?;
    let instance_path = parse_instance_path(args.instance_path)?;
    let change = match args.change {
        Change::Add => SelectionChange::Add,
        Change::Toggle => SelectionChange::Toggle,
        Change::Remove => SelectionChange::Remove,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ModifySelection {
            change,
            node: node_id(&args.node_id)?,
            instance_path,
        },
    ))
}

fn parse_select_by_rule(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SelectByRuleArgs = decode(arguments)?;
    let rule = match args.rule {
        Rule::All => SelectRule::All,
        Rule::Hierarchy => SelectRule::Hierarchy,
        Rule::Inverse => SelectRule::Inverse,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectByRule(rule),
    ))
}

fn parse_select_in_rect(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SelectInRectArgs = decode(arguments)?;
    let rect = NdcRect::new(
        unit_point("rect.min", args.rect.min)?,
        unit_point("rect.max", args.rect.max)?,
    )
    .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    let mode = match args.mode.unwrap_or(Marquee::Replace) {
        Marquee::Replace => MarqueeMode::Replace,
        Marquee::Add => MarqueeMode::Add,
        Marquee::Remove => MarqueeMode::Remove,
        Marquee::Toggle => MarqueeMode::Toggle,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectInRect { rect, mode },
    ))
}

fn parse_modify_component_selection(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ModifyComponentSelectionArgs = decode(arguments)?;
    if args.indices.len() > MAX_COMPONENT_INDICES {
        return Err(ErrorData::invalid_params(
            format!("indices exceeds {MAX_COMPONENT_INDICES} entries"),
            None,
        ));
    }
    let kind = match args.kind {
        Component::Face => ComponentKind::Face,
        Component::Vertex => ComponentKind::Vertex,
        Component::Edge => ComponentKind::Edge,
    };
    let mode = match args.mode.unwrap_or(Marquee::Replace) {
        Marquee::Replace => MarqueeMode::Replace,
        Marquee::Add => MarqueeMode::Add,
        Marquee::Remove => MarqueeMode::Remove,
        Marquee::Toggle => MarqueeMode::Toggle,
    };
    let (transform, shape, path) = component_ids(
        "Component",
        &args.owner.node_id,
        &args.owner.shape_id,
        &args.owner.instance_path,
    )?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ModifyComponentSelection {
            kind,
            mode,
            owner: ComponentOwner {
                transform,
                shape,
                path,
            },
            indices: args.indices,
        },
    ))
}

fn parse_deselect_all(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::DeselectAll)
}

fn parse_set_selection_mode(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SelectionModeArgs = decode(arguments)?;
    let mode = match args.mode {
        SelectionMode::Object => EditorMode::Object,
        SelectionMode::Face => EditorMode::Face,
        SelectionMode::Hierarchy => EditorMode::Hierarchy,
        SelectionMode::Vertex => EditorMode::Vertex,
        SelectionMode::Edge => EditorMode::Edge,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetSelectionMode(mode),
    ))
}

fn parse_select_face(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    if !arguments.contains_key("target") {
        return Err(ErrorData::invalid_params(
            "target must be a complete face target or null",
            None,
        ));
    }
    let args: SelectFaceArgs = decode(arguments)?;
    let target = args
        .target
        .map(|target| {
            if !(2..=257).contains(&target.instance_path.len()) {
                return Err(ErrorData::invalid_params(
                    "Face selection requires 2 to 257 path IDs",
                    None,
                ));
            }
            Ok(FaceTarget {
                transform: node_id(&target.node_id)?,
                shape: native_id(&target.shape_id)?,
                path: target
                    .instance_path
                    .iter()
                    .map(|value| native_id(value))
                    .collect::<Result<_, _>>()?,
                face_index: target.face_index,
            })
        })
        .transpose()?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectFace { target },
    ))
}

fn require_target_key(arguments: &JsonObject, noun: &str) -> Result<(), ErrorData> {
    if arguments.contains_key("target") {
        return Ok(());
    }
    Err(ErrorData::invalid_params(
        format!(
            "target must be a complete {} target or null",
            noun.to_ascii_lowercase()
        ),
        None,
    ))
}

fn component_ids(
    noun: &str,
    node: &str,
    shape: &str,
    instance_path: &[String],
) -> Result<(NodeId, NativeId, Vec<NativeId>), ErrorData> {
    if !(2..=257).contains(&instance_path.len()) {
        return Err(ErrorData::invalid_params(
            format!("{noun} selection requires 2 to 257 path IDs"),
            None,
        ));
    }
    Ok((
        node_id(node)?,
        native_id(shape)?,
        instance_path
            .iter()
            .map(|value| native_id(value))
            .collect::<Result<_, _>>()?,
    ))
}

fn parse_select_vertex(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    require_target_key(&arguments, "Vertex")?;
    let args: SelectVertexArgs = decode(arguments)?;
    let target = args
        .target
        .map(|target| {
            let (transform, shape, path) = component_ids(
                "Vertex",
                &target.node_id,
                &target.shape_id,
                &target.instance_path,
            )?;
            Ok(VertexTarget {
                transform,
                shape,
                path,
                vertex_index: target.vertex_index,
            })
        })
        .transpose()?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectVertex { target },
    ))
}

fn parse_select_edge(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    require_target_key(&arguments, "Edge")?;
    let args: SelectEdgeArgs = decode(arguments)?;
    let target = args
        .target
        .map(|target| {
            let (transform, shape, path) = component_ids(
                "Edge",
                &target.node_id,
                &target.shape_id,
                &target.instance_path,
            )?;
            Ok(EdgeTarget {
                transform,
                shape,
                path,
                edge_index: target.edge_index,
            })
        })
        .transpose()?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectEdge { target },
    ))
}

fn select_face_schema() -> JsonObject {
    let mut input = schema::<SelectFaceArgs>();
    input.insert("required".into(), json!(["scene_token", "target"]));
    input
}

fn select_vertex_schema() -> JsonObject {
    let mut input = schema::<SelectVertexArgs>();
    input.insert("required".into(), json!(["scene_token", "target"]));
    input
}

fn select_edge_schema() -> JsonObject {
    let mut input = schema::<SelectEdgeArgs>();
    input.insert("required".into(), json!(["scene_token", "target"]));
    input
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "create_primitive",
        description: "Create a cube, plane, sphere, cylinder, cone or torus in the live editor. Optional options set sizes, subdivisions and axis (x, y or z) for that primitive; omitted fields use the defaults, and fields of other primitives are rejected. Options are validated against a 20,000-face budget per primitive; larger subdivisions are refused with invalid_params",
        schema: schema::<CreateArgs>,
        parse: parse_create_primitive,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "select_node",
        description: "Select an object using an optional full transform instance_path; ambiguous objects require a path. Null node_id clears selection without a path",
        schema: schema::<SelectArgs>,
        parse: parse_select_node,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "modify_selection",
        description: "Add, toggle or remove one object instance in the ordered object selection (Shift, Ctrl and Shift+Ctrl clicks); the last member is the lead. add appends an absent instance as the lead or moves a present one to the end, toggle flips membership, remove drops a member and leaves an absent one unchanged; removing the lead promotes the previous member. Ambiguous objects require a full instance_path; Object and Hierarchy modes only. Not idempotent; does not edit the scene",
        schema: schema::<ModifySelectionArgs>,
        parse: parse_modify_selection,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "select_by_rule",
        description: "Select > All, Hierarchy or Inverse by rule. all selects every transform instance, nested ones included, with the last as the lead (only the roots in Hierarchy selection mode); hierarchy adds every descendant of the selected objects and keeps the lead (no change in Hierarchy selection mode); inverse selects the unselected instances (roots only in Hierarchy selection mode; an empty selection selects all). Object and Hierarchy modes only. Not idempotent because inverse flips; does not edit the scene",
        schema: schema::<SelectByRuleArgs>,
        parse: parse_select_by_rule,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "select_in_rect",
        description: "Marquee-select objects by a rectangle in normalised device coordinates (x right, y up, each within -1 to 1; corners in either order), the same typed operation as a viewport drag. mode replace (default, Maya select -replace) sets the selection to the hits, add appends the absent hits, remove (select -deselect) drops the hit members, toggle flips each hit; the hits are in instance order and the last appended or last hit is the lead. Object and Hierarchy modes only (Hierarchy maps hits to their roots). Any overlap of a visible mesh selects it; objects hidden behind others inside the rectangle ARE selected, as in Maya's object marquee (camera-based selection is documented for components only; PROVISIONAL, #30). Hidden instances and curves are not selectable. A zero-area rectangle (min equals max) picks like a click: the nearest hit only. Uses the aspect of the last painted frame (1 before the first), so read the camera with inspect_scene first. A replace with no hit clears the selection; add, remove and toggle with no hit change nothing. Not idempotent because toggle flips; does not edit the scene",
        schema: schema::<SelectInRectArgs>,
        parse: parse_select_in_rect,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "deselect_all",
        description: "Select > Deselect All (Alt+D): clear the object and any face, vertex or edge selection and return to Object selection mode, as Maya's SelectNone does. Session-only and idempotent; does not edit the scene",
        schema: schema::<MutationArgs>,
        parse: parse_deselect_all,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_selection_mode",
        description: "Set Object, Face, Vertex, Edge or Hierarchy (Root sub-mask only) selection mode without editing the scene; a valid remembered face, vertex or edge stays inactive outside its own mode",
        schema: schema::<SelectionModeArgs>,
        parse: parse_set_selection_mode,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "modify_component_selection",
        description: "Replace, add to, remove from or toggle the face, vertex or edge selection of the one selected object by component index (Maya select -replace, -add, -deselect, -toggle on f[n], vtx[n], e[n]); the same typed operation a component click or drag would use. owner names the selected object's shape as select_face does (node_id, shape_id and the full instance_path ending with the shape); indices are positions in that mesh, so a topology edit or Undo can renumber them, and members past the end are dropped. mode replace (default) sets the set to the indices in order (an empty list empties it), add appends the absent ones, remove drops the members, toggle flips each distinct index; repeated indices keep their first position and the last member is the lead (PROVISIONAL: Maya documents neither order nor lead). Requires the mode of kind (set_selection_mode) and refuses an owner that is not the single selected object without changing the selection, and any index outside the mesh without changing anything. Each type keeps its own set across mode switches; select_face, select_vertex and select_edge still replace the whole set with one component. Read the sets with inspect_scene component_selection. Not idempotent because add, remove and toggle depend on the set; does not edit the scene",
        schema: schema::<ModifyComponentSelectionArgs>,
        parse: parse_modify_component_selection,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "select_face",
        description: "Select a complete canonical face target in Face mode, or clear it with null; explicit targets need not be visible and select their owning instance",
        schema: select_face_schema,
        parse: parse_select_face,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "select_vertex",
        description: "Select a complete canonical vertex target in Vertex mode, or clear it with null; explicit targets need not be visible and select their owning instance",
        schema: select_vertex_schema,
        parse: parse_select_vertex,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "select_edge",
        description: "Select a complete canonical edge target in Edge mode, or clear it with null; explicit targets need not be visible and select their owning instance",
        schema: select_edge_schema,
        parse: parse_select_edge,
        idempotent: true,
        read_only: false,
    },
];
