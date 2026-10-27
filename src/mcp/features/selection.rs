use super::primitive_args::CreateArgs;
use crate::{
    editor_operations::{
        EdgeSelectionTarget, FaceSelectionTarget, Operation, RemoteRequest, SelectRule,
        SelectionChange, SelectionMode as EditorMode, VertexSelectionTarget,
    },
    mcp::{
        args::{MutationArgs, decode, mutation, native_id, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::{
    native_scene::{EdgeTarget, FaceTarget, NativeId, VertexTarget},
    scene::NodeId,
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
        description: "Create a cube, plane, sphere, cylinder or cone in the live editor. Optional options set sizes, subdivisions and axis (x, y or z) for that primitive; omitted fields use the defaults, and fields of other primitives are rejected. Options are validated against a 20,000-face budget per primitive; larger subdivisions are refused with invalid_params",
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
