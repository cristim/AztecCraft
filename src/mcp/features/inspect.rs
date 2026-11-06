use crate::{
    editor_operations::{InspectPage, Operation, RemoteRequest},
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
struct InspectArgs {
    /// Return at most this many nodes (1 to 4096), with the shapes and curves they hold. Omit it
    /// for one reply with the whole scene.
    #[schemars(range(min = 1, max = 4096))]
    node_limit: Option<u32>,
    /// The `page.next_cursor` of the previous paged reply; requires `node_limit`. It is opaque
    /// and valid only until the scene changes.
    cursor: Option<String>,
}

fn parse_inspect(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args = decode::<InspectArgs>(arguments)?;
    let page = match (args.node_limit, args.cursor) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(ErrorData::invalid_params(
                "cursor requires node_limit",
                None,
            ));
        }
        (Some(limit), cursor) => {
            // The schema range documents the bound; decoding does not enforce it.
            let max = azteccraft::native_scene::MAX_TRANSFORM_PATHS;
            let limit = usize::try_from(limit)
                .ok()
                .filter(|limit| (1..=max).contains(limit))
                .ok_or_else(|| {
                    ErrorData::invalid_params(format!("node_limit must be 1 to {max}"), None)
                })?;
            let cursor = cursor
                .map(|text| {
                    text.parse()
                        .map_err(|_| ErrorData::invalid_params("cursor is malformed", None))
                })
                .transpose()?;
            Some(InspectPage { limit, cursor })
        }
    };
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::Inspect { page },
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "inspect_scene",
    description: "Inspect committed editor state, including an active manipulation preview (Move, Rotate or Scale), document origin, bounded canonical curve IDs/names/class/counts/domain, retained-source counts and read-only reason, pending human edits and source-appearance readiness. A node's read-only `transform_attributes` (rotate order, rotate axis in degrees, shear, pivot translations and `non_default`) reports transform extras and is present only when at least one is non-default. Curve draw_available means at least one visible instance prepared as draw geometry; it does not claim GPU health or framebuffer success. `status_message` mirrors the native status line text (null when empty, capped at 512 characters, a fixed notice instead of file-error text that can carry a local path). `camera` reports the pose: target, eye, distance, yaw/pitch in degrees, vertical field of view at the current viewport aspect and clip planes. `manipulation.viewport` reports the last native pointer's pick: `pointer_hit` for the active tool's handle and `pointer_disabled` for a drawn-but-unsupported handle and `pointer_refusal` for why a Rotate or Scale press there would be refused (with a hit: that handle; with a null hit: the whole tool, for example a mirrored frame; null for Move), all null during an active manipulation. They mirror the painted highlight, so window focus, an open popup or a file modal do not null them; a hidden window has no `viewport`, and a view build error (for example an overflowing scale) nulls `viewport` from the first frame after the edit until a frame builds a view. No CV arrays or script bodies are exposed. A scene too large for one reply is read in pages: pass `node_limit` (50 to 100 suits most clients) and follow `page.next_cursor` while `page.truncated` is true; the cursor goes stale (`stale_scene`) as soon as the scene changes, and then the walk restarts without a cursor. Every page repeats the selection, camera and editor state; a shape or curve appears on the page of the node that holds it",
    schema: schema::<InspectArgs>,
    parse: parse_inspect,
    idempotent: false,
    read_only: true,
}];
