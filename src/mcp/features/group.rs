use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{
    GroupOptions, GroupPivot, GroupUnder, ReparentKeep, UngroupOptions,
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

fn preserve_by_default() -> bool {
    true
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum UnderInput {
    Parent,
    World,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum PivotInput {
    Center,
    Origin,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum KeepInput {
    World,
    Local,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GroupArgs {
    scene_token: String,
    /// `parent` (default): the group goes under the lowest common parent of the selected objects
    /// (an object's own parent for one object; the world for objects in different hierarchies).
    /// `world`: the group goes under the world.
    #[serde(default)]
    under: Option<UnderInput>,
    /// `center` (default, PROVISIONAL): the group's pivot is the centre of its members' bounding
    /// box. `origin`: the pivot stays at the group's origin.
    #[serde(default)]
    pivot: Option<PivotInput>,
    /// Keep every member's world transform (default true, Maya `-absolute`); false keeps the local
    /// values and the members move when the group's frame differs from their old parent's.
    #[serde(default = "preserve_by_default")]
    preserve_position: bool,
    /// The `selection_generation` that `inspect_scene` reported. With several objects selected
    /// the value is required, and a value that no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UngroupArgs {
    scene_token: String,
    /// `parent` (default): the children go to the group's parent. `world`: to the world.
    #[serde(default)]
    under: Option<UnderInput>,
    /// `world` (default, Maya `-absolute`): keep each child's world transform. `local`: keep the
    /// local values, so the children move when the new parent's frame differs.
    #[serde(default)]
    keep: Option<KeepInput>,
    /// The `selection_generation` that `inspect_scene` reported. With several objects selected
    /// the value is required, and a value that no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

fn under(input: Option<UnderInput>) -> GroupUnder {
    match input.unwrap_or(UnderInput::Parent) {
        UnderInput::Parent => GroupUnder::Parent,
        UnderInput::World => GroupUnder::World,
    }
}

fn parse_group_selection(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: GroupArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Group {
            options: GroupOptions {
                under: under(args.under),
                pivot: match args.pivot.unwrap_or(PivotInput::Center) {
                    PivotInput::Center => GroupPivot::Center,
                    PivotInput::Origin => GroupPivot::Origin,
                },
                preserve_position: args.preserve_position,
            },
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

fn parse_ungroup_selection(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: UngroupArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Ungroup {
            options: UngroupOptions {
                under: under(args.under),
                keep: match args.keep.unwrap_or(KeepInput::World) {
                    KeepInput::World => ReparentKeep::World,
                    KeepInput::Local => ReparentKeep::Local,
                },
            },
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "group_selection",
        description: "Edit > Group (Maya's group command) on the selected objects, as one undo step named Group. The top-most selected objects (a listed descendant is carried by its ancestor) become the last children of a new empty transform group<N> (the lowest free number), in selection order; the reply is created with the new group's node_id, and the group becomes the selection. under parent (the default) puts the group under the lowest common parent of the objects (an object's own parent for one object, the world for objects in different hierarchies), under world under the world. preserve_position true (the default, Maya -absolute) rewrites the members' local translation, rotation and scale so every world transform is unchanged, which needs no rewrite when the group's frame equals the old parent frame; false keeps the local values and the members shift. pivot center (the default, PROVISIONAL: the Group Options page names the choices, not the default) puts the group's pivots at the centre of the members' bounding box in the group's own space, origin leaves them at zero. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_reparent for an instanced object or, with preserve_position, a keyed, pivoted or extras node under a different parent frame, or a frame with shear, negative or zero scale (use preserve_position false); name_collision when two members share a name; resource_limit beyond the transform depth or path limits; read_only_format for Maya ASCII scenes and unsupported_edit for other opened files; stale_scene and busy_editing as for every edit. All or nothing: the scene, selection and undo stack are untouched on any refusal. Authored scenes with a hierarchy cannot be saved or recovered yet: Save As and autosave refuse them until the writers support hierarchies. No instancing, no empty group (-empty), no -name or -useAsGroup",
        schema: schema::<GroupArgs>,
        parse: parse_group_selection,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "ungroup_selection",
        description: "Edit > Ungroup (Maya's ungroup command) on the selected groups, as one undo step named Ungroup. The children of each top-most selected group (a listed descendant is carried by its ancestor, as Hierarchy selection lists them) move to the group's parent (under parent, the default) or to the world (under world) as last children, and the emptied group is deleted with its keys; the reply is nodes with the children's node_ids in order, and they become the selection. keep world (the default, Maya -absolute) rewrites each child's local values so its world transform is unchanged, which needs no rewrite when the frames agree; keep local keeps the local values and the children move. A target must be a plain group: a transform with a shape or a joint is refused with invalid_reparent. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Also refused with invalid_reparent for an instanced child or, with keep world, a keyed, pivoted or extras child under a different frame, or a frame with shear, negative or zero scale (use keep local); name_collision when a child's name already exists among its new siblings (Maya renames, PROVISIONAL difference); resource_limit; read_only_format for Maya ASCII scenes and unsupported_edit for other opened files; stale_scene and busy_editing as for every edit. All or nothing: the scene, selection and undo stack are untouched on any refusal",
        schema: schema::<UngroupArgs>,
        parse: parse_ungroup_selection,
        idempotent: false,
        read_only: false,
    },
];
