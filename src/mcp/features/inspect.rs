use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, schema},
        registry::ToolSpec,
    },
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectArgs {}

fn parse_inspect(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    decode::<InspectArgs>(arguments)?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::Inspect,
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "inspect_scene",
    description: "Inspect committed editor state, including an active manipulation preview (Move, Rotate or Scale), document origin, bounded canonical curve IDs/names/class/counts/domain, retained-source counts and read-only reason, pending human edits and source-appearance readiness. A node's read-only `transform_attributes` (rotate order, rotate axis in degrees, shear, pivot translations and `non_default`) reports transform extras and is present only when at least one is non-default. Curve draw_available means at least one visible instance prepared as draw geometry; it does not claim GPU health or framebuffer success. `status_message` mirrors the native status line text (null when empty, capped at 512 characters, a fixed notice instead of file-error text that can carry a local path). `camera` reports the pose: target, eye, distance, yaw/pitch in degrees, vertical field of view at the current viewport aspect and clip planes. `manipulation.viewport` reports the last native pointer's pick: `pointer_hit` for the active tool's handle and `pointer_disabled` for a drawn-but-unsupported handle, both null during an active manipulation. No CV arrays or script bodies are exposed",
    schema: schema::<InspectArgs>,
    parse: parse_inspect,
    idempotent: false,
    read_only: true,
}];
