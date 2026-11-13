use crate::{
    editor_operations::{Operation, RemoteRequest, TransformInput},
    mcp::{
        args::{decode, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::ReparentKeep;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenameArgs {
    scene_token: String,
    node_id: String,
    name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransformArgs {
    scene_token: String,
    node_id: String,
    translation: [f64; 3],
    rotation_degrees: [f64; 3],
    scale: [f64; 3],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EditNodeArgs {
    scene_token: String,
    node_id: String,
    name: Option<String>,
    #[serde(default)]
    translation: [Option<f64>; 3],
    #[serde(default)]
    rotation_degrees: [Option<f64>; 3],
    #[serde(default)]
    scale: [Option<f64>; 3],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ParentArgs {
    scene_token: String,
    node_id: String,
    /// The new parent. Give exactly one of `parent_id` and `world`.
    parent_id: Option<String>,
    /// True to unparent the object to the world (Maya `parent -world`). Give exactly one of `parent_id` and `world`.
    world: Option<bool>,
    /// "world" (default, Maya `-absolute`) rewrites the local channels so the world transform is unchanged; "local" (Maya `-relative`) keeps them so the object moves in world space.
    keep: Option<KeepKind>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum KeepKind {
    World,
    Local,
}

fn parse_parent_node(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ParentArgs = decode(arguments)?;
    let parent = match (args.parent_id, args.world) {
        (Some(id), None | Some(false)) => Some(node_id(&id)?),
        (None, Some(true)) => None,
        _ => {
            return Err(ErrorData::invalid_params(
                "give exactly one of parent_id and world=true",
                None,
            ));
        }
    };
    let keep = match args.keep.unwrap_or(KeepKind::World) {
        KeepKind::World => ReparentKeep::World,
        KeepKind::Local => ReparentKeep::Local,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Reparent {
            id: node_id(&args.node_id)?,
            parent,
            keep,
        },
    ))
}

fn parse_rename_node(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: RenameArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Rename {
            id: node_id(&args.node_id)?,
            name: args.name,
        },
    ))
}

fn parse_set_transform(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: TransformArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetTransform {
            id: node_id(&args.node_id)?,
            transform: TransformInput {
                translation: args.translation,
                rotation_degrees: args.rotation_degrees,
                scale: args.scale,
            },
        },
    ))
}

fn parse_edit_node(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: EditNodeArgs = decode(arguments)?;
    if args
        .translation
        .iter()
        .chain(&args.rotation_degrees)
        .chain(&args.scale)
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(ErrorData::invalid_params(
            "Channel values must be finite",
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::EditNode {
            id: node_id(&args.node_id)?,
            name: args.name,
            translation: args.translation,
            rotation_degrees: args.rotation_degrees,
            scale: args.scale,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "rename_node",
        description: "Rename an existing object",
        schema: schema::<RenameArgs>,
        parse: parse_rename_node,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "parent_node",
        description: "Parent one object under another, or unparent it to the world with world=true, as one undo step (Maya's parent command). keep world (the default, Maya -absolute) rewrites the object's local translation, rotation and scale so its world transform is unchanged; keep local (Maya -relative) leaves them and the object moves in world space. The object becomes the new parent's last child; the same parent is unchanged. Refused with invalid_reparent for a cycle (under itself or a descendant), an instanced object, or keep world on a node with non-default pivots, extras or keys or when no translate, XYZ rotate and scale keeps the world transform (shear, negative or zero scale; use keep local); name_collision when the name already exists among the new siblings (Maya renames instead, PROVISIONAL difference); resource_limit when the result exceeds the transform depth or path limits; scene_error for an unknown node; read_only_format for Maya ASCII scenes and unsupported_edit for other opened files; stale_scene and busy_editing as for every edit. A rotation rewritten by keep world is in (-180, 180] degrees (PROVISIONAL). Selected objects stay selected and follow to their new instance paths (in Hierarchy selection mode that can leave a non-root member, PROVISIONAL). The Parent and Unparent undo labels are PROVISIONAL (the docs name none). Authored scenes with a hierarchy cannot be saved yet",
        schema: schema::<ParentArgs>,
        parse: parse_parent_node,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "set_transform",
        description: "Set f64 translation in declared scene units, rotation in XYZ degrees, and scale",
        schema: schema::<TransformArgs>,
        parse: parse_set_transform,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "edit_node",
        description: "Edit optional name and individual f64 channels in one undo entry. Omitted arrays and null components are untouched; translation uses declared scene units and supplied rotations use degrees. Does not change selection",
        schema: schema::<EditNodeArgs>,
        parse: parse_edit_node,
        idempotent: false,
        read_only: false,
    },
];
