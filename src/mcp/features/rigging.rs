use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::DEFAULT_JOINT_RADIUS;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateJointArgs {
    scene_token: String,
    /// The joint to parent under; omit it to start a new hierarchy. The selection is not read.
    parent_id: Option<String>,
    /// World position, or the offset from the parent when `relative` is true. Default the origin.
    position: Option<[f64; 3]>,
    /// Measure `position` from the parent. Without a parent it changes nothing. Default false.
    #[serde(default)]
    relative: bool,
    /// Greater than 0 and at most 1000000. Default 1.
    radius: Option<f64>,
    /// Omit it for the lowest free `joint<N>`.
    name: Option<String>,
}

fn parse_create_joint(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: CreateJointArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::CreateJoint {
            parent: args.parent_id.as_deref().map(node_id).transpose()?,
            position: args.position.unwrap_or_default(),
            relative: args.relative,
            radius: args.radius.unwrap_or(DEFAULT_JOINT_RADIUS),
            name: args.name,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "create_joint",
    description: "Create a session-only joint, the same operation as Skeleton > Create Joints: a bare transform that stays selected, with rotation 0 and scale 1. `parent_id` must name a joint and is the only way to attach it; the selection is not read. A world `position` under a parent is solved through the parent's world matrix and refused when that matrix is singular; `relative` stores the position as given. A joint under a joint keeps segment scale compensation on. A scene holding joints cannot be saved, and joints are not written to any file. Refused on an opened scene, in a component selection mode, for a parent that is not a joint, a radius of 0 or less or above 1000000, an invalid or colliding name and past the hierarchy depth limit. `inspect_scene` reports `joint.radius` on each joint; `delete_node` removes a joint with everything below it, as one undo step",
    schema: schema::<CreateJointArgs>,
    parse: parse_create_joint,
    idempotent: false,
    read_only: false,
}];
