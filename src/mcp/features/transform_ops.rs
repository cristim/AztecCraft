use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{AlignMode, AlignOptions, AlignTo, TransformChannels};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

fn on_by_default() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransformationArgs {
    scene_token: String,
    /// Include Translate (default true).
    #[serde(default = "on_by_default")]
    translate: bool,
    /// Include Rotate (default true).
    #[serde(default = "on_by_default")]
    rotate: bool,
    /// Include Scale (default true).
    #[serde(default = "on_by_default")]
    scale: bool,
    /// The `selection_generation` that `inspect_scene` reported. With several objects selected
    /// the value is required, and a value that no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

impl TransformationArgs {
    fn channels(&self) -> TransformChannels {
        TransformChannels {
            translate: self.translate,
            rotate: self.rotate,
            scale: self.scale,
        }
    }
}

fn parse_reset(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: TransformationArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ResetTransformations {
            channels: args.channels(),
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

fn parse_freeze(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: TransformationArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::FreezeTransformations {
            channels: args.channels(),
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

fn parse_match(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: TransformationArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::MatchTransformations {
            channels: args.channels(),
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AlignModeInput {
    None,
    Min,
    Mid,
    Max,
    Dist,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AlignToInput {
    SelectionAverage,
    LastSelected,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AlignObjectsArgs {
    scene_token: String,
    /// What the world X axis does: none (default), min, mid, max or dist.
    #[serde(default = "no_align")]
    x: AlignModeInput,
    /// What the world Y axis does: none (default), min, mid, max or dist.
    #[serde(default = "no_align")]
    y: AlignModeInput,
    /// What the world Z axis does: none (default), min, mid, max or dist.
    #[serde(default = "no_align")]
    z: AlignModeInput,
    /// The reference for min, mid and max: selection_average (default) or last_selected.
    #[serde(default = "average")]
    align_to: AlignToInput,
    /// The `selection_generation` that `inspect_scene` reported. With several objects selected
    /// the value is required, and a value that no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

fn no_align() -> AlignModeInput {
    AlignModeInput::None
}

fn average() -> AlignToInput {
    AlignToInput::SelectionAverage
}

fn parse_align(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: AlignObjectsArgs = decode(arguments)?;
    let mode = |input: AlignModeInput| match input {
        AlignModeInput::None => AlignMode::None,
        AlignModeInput::Min => AlignMode::Min,
        AlignModeInput::Mid => AlignMode::Mid,
        AlignModeInput::Max => AlignMode::Max,
        AlignModeInput::Dist => AlignMode::Dist,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::AlignObjects {
            options: AlignOptions {
                modes: [mode(args.x), mode(args.y), mode(args.z)],
                to: match args.align_to {
                    AlignToInput::SelectionAverage => AlignTo::SelectionAverage,
                    AlignToInput::LastSelected => AlignTo::LastSelected,
                },
            },
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "reset_transformations",
        description: "Modify > Reset Transformations on every selected object, in one undo entry named Reset Transformations: the chosen channels (translate, rotate, scale; all true by default) return to 0, 0 and 1. Children are not compensated and follow their parent. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_transform_edit for no channel, a joint or a keyed object; a Maya ASCII scene is read-only (read_only_format). All or nothing, with the scene and history untouched on any refusal. The Joint orient, Lock Normals and Preserve Normals options are unsupported (PROVISIONAL: option defaults and shear and rotate-order cases are not compared with Maya)",
        schema: schema::<TransformationArgs>,
        parse: parse_reset,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "freeze_transformations",
        description: "Modify > Freeze Transformations on every selected object, in one undo entry named Freeze Transformations: the chosen channels (translate, rotate, scale; all true by default) of each selected object and every transform below it are reset to their defaults and pushed into the meshes beneath, so every point keeps its world position. A partial freeze leaves every unchosen channel exactly as stored and bakes the difference into the points (which can carry shear when scale stays non-uniform; PROVISIONAL, not compared with Maya); all three channels false is refused as invalid_transform_edit, unlike the native none-chosen-means-all path. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_transform_edit for no channel, a joint, a keyed object, an instanced object, non-default pivots or transform extras, a curve below the object, or a mirroring (negative), zero or near-singular scale; a Maya ASCII scene or a sourced scene is refused (read_only_format, unsupported_edit). All or nothing, with the scene and history untouched on any refusal. Normals are derived from the points, so Lock and Preserve Normals and Joint orient are unsupported (PROVISIONAL, not compared with Maya makeIdentity)",
        schema: schema::<TransformationArgs>,
        parse: parse_freeze,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "match_transformations",
        description: "Modify > Match Transformations on the object selection, in one undo entry named Match Transformations: every selected object except the last selected (the target, the lead) takes the target's WORLD transform for the chosen channels (translate, rotate, scale; all true by default), written as new local channels under each object's parent. Unchosen channels keep their stored bits. Children are not compensated and move with their parent. Objects are matched shallowest first, so a selected descendant of a selected object also ends on the target. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_transform_edit for fewer than two objects, no channel (all three false, unlike the native none-chosen-means-all path), a target below a selected object, a joint, a keyed object, an instanced object, non-default pivots or transform extras on the target or a moved object, a zero-scaled parent, or a result the stored translate, rotate and scale channels cannot hold exactly (shear, mirroring, or a rotation under a non-uniformly scaled parent that would also need a different scale: refused, never approximated); a Maya ASCII scene or a sourced scene is refused (read_only_format, unsupported_edit), and so is a scene with non-default units (unsupported_edit). All or nothing, with the scene and history untouched on any refusal. Match Pivots and bounding-box matching are unsupported (PROVISIONAL, not compared with Maya matchTransform: the docs do not state world versus local space or child handling)",
        schema: schema::<TransformationArgs>,
        parse: parse_match,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "align_objects",
        description: "Modify > Snap Align Objects > Align Objects on the object selection, in one undo entry named Align Objects: the selected objects move along the WORLD axes so their world bounding boxes line up. The box of an object is the axis-aligned world box of every mesh point at or below it. Per axis x, y and z choose none (default, the axis is untouched), min (the box side closest to 0 on that axis), mid (the box centres), max (the side farthest from 0) or dist (the outermost objects by box centre stay and the gaps between the boxes become equal). align_to (default selection_average) is the reference for min, mid and max: the average of the selected boxes' value, or last_selected, the last selected object's box, which does not move; dist ignores it. Only each moved object's translate channels are written (through its parent's inverse, so rotated or scaled parents are fine); rotate and scale keep their bits and children follow their parent. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_transform_edit for fewer than two objects, all three axes none, a joint, a keyed object, an instanced object, non-default pivots or transform extras, an object with no mesh below it (no bounding box), a curve below an object, a selected object below another selected object, a zero-scaled parent, or a result the stored translate cannot hold exactly; a Maya ASCII scene or a sourced scene is refused (read_only_format, unsupported_edit), and so is a scene with non-default units (unsupported_edit). All or nothing, with the scene and history untouched on any refusal. Stack, the selection min and max and world origin references and the Align Tool are unsupported. PROVISIONAL (#5, not compared with Maya): the fetched Maya 2027 Align Objects pages do not state how rotated or scaled objects' boxes are computed, any defaults or how dist distributes, so the world box of the mesh points, the defaults and the dist rule are this editor's choice",
        schema: schema::<AlignObjectsArgs>,
        parse: parse_align,
        idempotent: true,
        read_only: false,
    },
];
