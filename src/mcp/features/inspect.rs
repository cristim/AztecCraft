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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectLimitsArgs {}

fn parse_inspect_limits(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    decode::<InspectLimitsArgs>(arguments)?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::InspectLimits,
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "inspect_scene",
        description: "Inspect committed editor state, including an active manipulation preview (Move, Rotate or Scale; a component Move in Vertex, Edge or Face selection reports kind move_components with component_kind, vertex_count and null channels, baseline_channels and current_channels), document origin, bounded canonical curve IDs/names/class/counts/domain, retained-source counts and read-only reason, pending human edits and source-appearance readiness. A node's read-only `transform_attributes` (rotate order, rotate axis in degrees, shear, rotate and scale pivots, pivot translations, `inherits_transform`, `offset_parent_matrix` (row-vector as stored, translation row in the scene's linear unit, omitted when it is the identity; the `inherits_transform` false plus offset combination is PROVISIONAL, unverified against Maya) and `non_default`) reports transform extras and is present only when at least one is non-default. Curve draw_available means at least one visible instance prepared as draw geometry; it does not claim GPU health or framebuffer success. `status_message` mirrors the native status line text (null when empty, capped at 512 characters, a fixed notice instead of file-error text that can carry a local path). `camera` reports the view (persp, top, front, side or ortho), the projection (perspective or orthographic) and the pose: target, eye, distance, yaw/pitch in degrees, vertical field of view at the current viewport aspect and clip planes, `focal_length_mm` (null for a lens specified only by field of view), `blocked_cameras` (names of cameras whose film back the opened file states in a way the editor cannot carry: the viewport shows its default lens for them), `film_back` (the stored film back in the units and names of set_camera_film_back, null for a field-of-view lens), `image_plane` (null without a plane; else the `set_image_plane` settings with `depth_cm` in centimetres, `image_size`, `resolved_file_name` (the file name only, never the directory), `resolved_frame` (current time plus `frame_offset` while `use_frame_extension` is on, else null), `frame_state` (`not_loaded` before the editor has requested the frame, which includes the moment after a time change until the next UI tick even when the frame is cached, else `loading`, `ready`, `missing` or `failed`; the same state the status line and the viewport use, PROVISIONAL naming), `frame_error` (when failed, one of `io`, `too_large`, `header_too_large`, `unsupported_format`, `corrupt`, `truncated`, `dimension_mismatch`, `internal`, `worker_gone`, `missing`) and `cache` (`frames` and `bytes` of decoded frames held, `budget_bytes` the policy cap)), and `center_of_interest_cm` (eye-to-target distance). `can_undo_view` and `can_redo_view` report session camera-history availability. `native_gesture_active` reports a held native camera gesture that blocks remote mutation even without new pointer events. These describe the session viewport camera, not imported camera nodes. `manipulation.viewport` reports the last native pointer's pick: `pointer_hit` for the active tool's handle and `pointer_disabled` for a drawn-but-unsupported handle and `pointer_refusal` for why a Rotate or Scale press there would be refused (with a hit: that handle; with a null hit: the whole tool, for example a mirrored frame; null for Move), all null during an active manipulation. They mirror the painted highlight, so window focus, an open popup or a file modal do not null them; a hidden window has no `viewport`, and a view build error (for example an overflowing scale) nulls `viewport` from the first frame after the edit until a frame builds a view. No CV arrays or script bodies are exposed. A scene too large for one reply is read in pages: pass `node_limit` (50 to 100 suits most clients) and follow `page.next_cursor` while `page.truncated` is true; the cursor goes stale (`stale_scene`) as soon as the scene changes, and then the walk restarts without a cursor. Every page repeats the selection, camera and editor state; a shape or curve appears on the page of the node that holds it",
        schema: schema::<InspectArgs>,
        parse: parse_inspect,
        idempotent: false,
        read_only: true,
    },
    ToolSpec {
        name: "inspect_limits",
        description: "Read the resource limits AztecCraft enforces when it opens, imports, creates, recovers or builds a view of a scene: input byte caps, parser statement, token and work caps, instance path and depth caps, primitive face and division caps, and the derived view and recovery byte budgets. Each row has `name`, `value`, `unit` (`bytes`, `count` or `work_steps`, which counts metered steps and is not time) and `enforced_by`, the owning constant. `limits` is static for the build. `usage` reads the committed scene: `statistics` (counts of `nodes`, `shapes`, `curves`, `joints`, `keys`, `vertices`, `edges`, `faces`, `triangles` and `control_vertices`, counted per record, so a node or shape instanced under several parents counts once) and `caps`, one row per limit that bounds the live scene (`instance_paths`, `instance_depth`, `instance_path_identities`, `view_bytes`) with `limit`, `used`, `headroom` (never negative) and `estimate` (`measured`, `saturated` for a view figure that overflows, or `unavailable` with null `used` and `headroom`). `view_bytes.used` is a LOWER BOUND of what the view build charges (its preflight figure, without curve tessellation), so a scene with many curves can show headroom and still be refused; it is not process memory. `instance_paths`, `instance_depth` and `instance_path_identities` count instance paths and are `unavailable` when the scene exceeds the cap. It needs no token and passes no guard. The values are PROVISIONAL application policy: Maya states no scene limits. Exceeding one makes the operation fail with a typed refusal and leaves the scene unchanged",
        schema: schema::<InspectLimitsArgs>,
        parse: parse_inspect_limits,
        idempotent: false,
        read_only: true,
    },
];
