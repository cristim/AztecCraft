use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{ALL_CHANNELS, ChannelMask};
use azteccraft::scene::NodeId;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

/// Most objects one key request names, like the selection's own bound.
const MAX_KEY_NODES: usize = 4096;

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum KeyChannel {
    TranslateX,
    TranslateY,
    TranslateZ,
    RotateX,
    RotateY,
    RotateZ,
    ScaleX,
    ScaleY,
    ScaleZ,
}

fn channel_mask(channels: Option<Vec<KeyChannel>>) -> ChannelMask {
    let Some(channels) = channels else {
        return ALL_CHANNELS;
    };
    let mut mask = [false; 9];
    for channel in channels {
        mask[channel as usize] = true;
    }
    mask
}

fn node_ids(ids: Option<Vec<String>>) -> Result<Option<Vec<NodeId>>, ErrorData> {
    let Some(ids) = ids else {
        return Ok(None);
    };
    if ids.len() > MAX_KEY_NODES {
        return Err(ErrorData::invalid_params("too many node_ids", None));
    }
    ids.iter()
        .map(|id| node_id(id))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetCurrentTimeArgs {
    scene_token: String,
    /// A whole frame.
    time: f64,
}

fn parse_set_current_time(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetCurrentTimeArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetCurrentTime { time: args.time },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetPlaybackRangeArgs {
    scene_token: String,
    /// A whole frame, not after `end`.
    start: f64,
    /// A whole frame.
    end: f64,
}

fn parse_set_playback_range(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetPlaybackRangeArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetPlaybackRange {
            start: args.start,
            end: args.end,
        },
    ))
}

fn parse_play(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::Play)
}

fn parse_stop(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::Stop)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct KeyArgs {
    scene_token: String,
    /// The objects to act on. Omit it for the object selection.
    #[schemars(length(min = 1, max = 4096))]
    node_ids: Option<Vec<String>>,
    /// A whole frame.
    time: Option<f64>,
    /// Omit it for all nine transform channels.
    #[schemars(length(min = 1, max = 9))]
    channels: Option<Vec<KeyChannel>>,
}

fn parse_set_keyframe(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: KeyArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetKeyframe {
            nodes: node_ids(args.node_ids)?,
            time: args.time,
            channels: channel_mask(args.channels),
        },
    ))
}

fn parse_delete_keys(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: KeyArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::DeleteKeys {
            nodes: node_ids(args.node_ids)?,
            time: args.time,
            channels: channel_mask(args.channels),
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectKeysArgs {
    /// Read only this object's keys.
    node_id: Option<String>,
    /// The `next_cursor` of the previous reply, for the same `node_id`. It is opaque and valid
    /// only until the scene changes.
    cursor: Option<String>,
}

fn parse_inspect_keys(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: InspectKeysArgs = decode(arguments)?;
    let cursor = args
        .cursor
        .map(|text| {
            text.parse()
                .map_err(|_| ErrorData::invalid_params("cursor is malformed", None))
        })
        .transpose()?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::InspectKeys {
            node: args.node_id.as_deref().map(node_id).transpose()?,
            cursor,
        },
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_current_time",
        description: "Move the current time to a whole frame and write every keyed channel's value there. This is not an undo step and does not dirty a scene with no keys. Fractional times are refused with invalid_frame. A human draft refuses it with busy_editing, and a manipulation with busy_manipulation. A manual edit of a keyed channel holds until the next time change, then snaps back to the keys (PROVISIONAL). Stops playback",
        schema: schema::<SetCurrentTimeArgs>,
        parse: parse_set_current_time,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_keyframe",
        description: "Key transform channels with their current value, as one undo step. No time means the current time, no node_ids means the object selection (Object or Hierarchy mode), and no channels means all nine transform channels. Keys are session-only whole-frame values, interpolated linearly and held outside them (PROVISIONAL: tangents are not implemented). Rotation values are in the scene's declared angular unit. Sourced scenes refuse keys, and a scene holding keys cannot be saved. Bounded by key_limit. Stops playback",
        schema: schema::<KeyArgs>,
        parse: parse_set_keyframe,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "delete_keys",
        description: "Delete keys as one undo step: at `time`, or at every frame when it is omitted. No node_ids means the object selection, and no channels means all nine. Remaining keys are evaluated at the current time; a channel left with no key keeps its pose. Stops playback",
        schema: schema::<KeyArgs>,
        parse: parse_delete_keys,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_playback_range",
        description: "Set the playback range in whole frames. Session-only, not an undo step. The start must not be after the end (invalid_playback_range). Stops playback",
        schema: schema::<SetPlaybackRangeArgs>,
        parse: parse_set_playback_range,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "play_timeline",
        description: "Start playback at 24 fps, looping continuously over the playback range, skipping frames rather than slowing down (rate and loop PROVISIONAL). A time outside the range starts at the range start. It advances only while the native window renders. It stops on a manipulation, a focused text field, a draft, a file modal, close or any mutating operation; read `timeline.playing`. Each frame that moves a keyed channel changes the revision, so paged cursors go stale during playback",
        schema: schema::<MutationArgs>,
        parse: parse_play,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "stop_timeline",
        description: "Stop playback, leaving the current time where it is",
        schema: schema::<MutationArgs>,
        parse: parse_stop,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "inspect_keys",
        description: "Read stored keys in node, channel and frame order, optionally for one object. On a sourced scene `unavailable_reason` says source animation curves are not exposed or evaluated, which does not mean there are none. A large table is read in pages: follow `next_cursor` until it is null; it goes stale (`stale_scene`) as soon as the scene changes, including on playback ticks",
        schema: schema::<InspectKeysArgs>,
        parse: parse_inspect_keys,
        idempotent: false,
        read_only: true,
    },
];
