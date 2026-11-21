use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::TransformChannels;
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

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "reset_transformations",
        description: "Modify > Reset Transformations on every selected object, in one undo entry named Reset Transformations: the chosen channels (translate, rotate, scale; all true by default) return to 0, 0 and 1. Children are not compensated and follow their parent. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_transform_edit for no channel, a joint or a keyed object; a Maya ASCII scene is read-only (read_only_format). All or nothing, with the scene and history untouched on any refusal. Match Transformations and the Joint orient, Lock Normals and Preserve Normals options are unsupported (PROVISIONAL: option defaults and shear and rotate-order cases are not compared with Maya)",
        schema: schema::<TransformationArgs>,
        parse: parse_reset,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "freeze_transformations",
        description: "Modify > Freeze Transformations on every selected object, in one undo entry named Freeze Transformations: the chosen channels (translate, rotate, scale; all true by default) of each selected object and every transform below it are reset to their defaults and pushed into the meshes beneath, so every point keeps its world position. A partial freeze needs translate, XYZ rotate and positive scale; otherwise freeze all three. Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. Refused with invalid_transform_edit for no channel, a joint, a keyed object, an instanced object, non-default pivots or transform extras, a curve below the object, a mirroring (negative) or zero scale, or a partial freeze that cannot be decomposed; a Maya ASCII scene or a sourced scene is refused (read_only_format, unsupported_edit). All or nothing, with the scene and history untouched on any refusal. Normals are derived from the points, so Lock and Preserve Normals and Joint orient are unsupported (PROVISIONAL, not compared with Maya makeIdentity)",
        schema: schema::<TransformationArgs>,
        parse: parse_freeze,
        idempotent: true,
        read_only: false,
    },
];
