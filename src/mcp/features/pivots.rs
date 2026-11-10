use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::PivotReset;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

fn preserve_by_default() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetPivotsArgs {
    scene_token: String,
    node_id: String,
    /// New rotate pivot in declared scene units, Object (local) space. Omit to leave it alone.
    rotate_pivot: Option<[f64; 3]>,
    /// New scale pivot in declared scene units, Object (local) space. Omit to leave it alone.
    scale_pivot: Option<[f64; 3]>,
    /// Keep the node's matrix by compensating through the rotate and scale pivot translates
    /// (default true, like `xform -preserve`); false writes the raw pivots and the object may move.
    #[serde(default = "preserve_by_default")]
    preserve: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PivotModeInput {
    Center,
    Zero,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResetPivotsArgs {
    scene_token: String,
    mode: PivotModeInput,
    /// The `selection_generation` that `inspect_scene` reported. With several objects selected
    /// the value is required, and a value that no longer matches the selection is refused.
    #[serde(default)]
    expected_selection_generation: Option<u64>,
}

fn parse_set_pivots(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetPivotsArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetPivots {
            id: node_id(&args.node_id)?,
            rotate_pivot: args.rotate_pivot,
            scale_pivot: args.scale_pivot,
            preserve: args.preserve,
        },
    ))
}

fn parse_reset_pivots(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ResetPivotsArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ResetPivots {
            mode: match args.mode {
                PivotModeInput::Center => PivotReset::Center,
                PivotModeInput::Zero => PivotReset::Zero,
            },
            expected_selection_generation: args.expected_selection_generation,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_pivots",
        description: "Set the rotate and scale pivots of one node in one undo entry named Pivot. Pivots are f64 triples in declared scene units, Object (local) space; World space is unsupported. An omitted pivot is untouched, and both omitted only checks the node and returns unchanged. With preserve true (the default, like xform -preserve; the placement is PROVISIONAL) the node's matrix is kept by compensating through its rotate and scale pivot translates, so the object does not move; with preserve false the raw pivots are written and a rotated or scaled object moves. Does not change selection. On an opened Maya Binary scene a preserve edit is accepted only when no compensation is needed (otherwise pivot_needs_translate_correction), and a node without the rotate or scale pivot record refuses a nonzero value (absent_pivot_leaf, or unsupported_edit and insertion_after_delete); Maya ASCII is read-only (read_only_format). A node with session keys refuses (pivot_on_keyed_node). A non-finite input is a protocol error and a value that overflows when applied is invalid_input; nothing changes on any refusal",
        schema: schema::<SetPivotsArgs>,
        parse: parse_set_pivots,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "reset_pivots",
        description: "Modify > Center Pivot (mode center) or Zero Pivot (mode zero) on every selected object, each handled independently, in one undo entry named Pivot; the object does not move. Center puts both pivots at the centre of the node's hierarchy bounding box in its own Object space, over visible mesh and curve control points (PROVISIONAL); a node with no geometry stays unchanged, and unproved geometry visibility refuses the call with scene_error. Zero puts both pivots at the node's origin and keeps the matrix (PROVISIONAL, follows xform -zeroTransformPivots). Pass expected_selection_generation from inspect_scene; it is required with several objects selected and a mismatch is stale_selection. Object and Hierarchy selection only: component modes return invalid_input and an empty selection returns no_selection. A node with session keys refuses (pivot_on_keyed_node), as do the same Maya Binary and Maya ASCII cases as set_pivots; all or nothing, with the scene and history untouched on any refusal. Component Center Pivot and Bake Pivot are unsupported",
        schema: schema::<ResetPivotsArgs>,
        parse: parse_reset_pivots,
        idempotent: false,
        read_only: false,
    },
];
