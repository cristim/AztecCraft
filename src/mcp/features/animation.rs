use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{ALL_CHANNELS, ChannelMask, Infinity};
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

/// The schema allows one to nine distinct channels, so the parser refuses what it forbids.
fn channel_mask(channels: Option<Vec<KeyChannel>>) -> Result<ChannelMask, ErrorData> {
    let Some(channels) = channels else {
        return Ok(ALL_CHANNELS);
    };
    if channels.is_empty() || channels.len() > ALL_CHANNELS.len() {
        return Err(ErrorData::invalid_params(
            "channels must name one to nine channels",
            None,
        ));
    }
    let mut mask = [false; 9];
    for channel in channels {
        if std::mem::replace(&mut mask[channel as usize], true) {
            return Err(ErrorData::invalid_params(
                "channels must not repeat a channel",
                None,
            ));
        }
    }
    Ok(mask)
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
    #[schemars(length(min = 1, max = 9), extend("uniqueItems" = true))]
    channels: Option<Vec<KeyChannel>>,
}

fn parse_set_keyframe(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: KeyArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetKeyframe {
            nodes: node_ids(args.node_ids)?,
            time: args.time,
            channels: channel_mask(args.channels)?,
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
            channels: channel_mask(args.channels)?,
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EulerFilterArgs {
    scene_token: String,
    /// The objects to filter. Omit it for the object selection.
    #[schemars(length(min = 1, max = 4096))]
    node_ids: Option<Vec<String>>,
    /// Rotation channels only. Omit it for rotate_x, rotate_y and rotate_z.
    #[schemars(length(min = 1, max = 3), extend("uniqueItems" = true))]
    channels: Option<Vec<KeyChannel>>,
}

fn parse_euler_filter(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: EulerFilterArgs = decode(arguments)?;
    let channels = match args.channels {
        Some(channels) => channel_mask(Some(channels))?,
        None => [false, false, false, true, true, true, false, false, false],
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::EulerFilter {
            nodes: node_ids(args.node_ids)?,
            channels,
        },
    ))
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum InfinityMode {
    Constant,
    Linear,
    Cycle,
    CycleRelative,
    Oscillate,
}

impl From<InfinityMode> for Infinity {
    fn from(mode: InfinityMode) -> Self {
        match mode {
            InfinityMode::Constant => Self::Constant,
            InfinityMode::Linear => Self::Linear,
            InfinityMode::Cycle => Self::Cycle,
            InfinityMode::CycleRelative => Self::CycleRelative,
            InfinityMode::Oscillate => Self::Oscillate,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetInfinityArgs {
    scene_token: String,
    /// The objects to act on. Omit it for the object selection.
    #[schemars(length(min = 1, max = 4096))]
    node_ids: Option<Vec<String>>,
    /// Omit it for all nine channels. A channel without a key is skipped.
    #[schemars(length(min = 1, max = 9), extend("uniqueItems" = true))]
    channels: Option<Vec<KeyChannel>>,
    /// Behaviour before the first key. Omit it to leave it as it is.
    pre: Option<InfinityMode>,
    /// Behaviour after the last key. Omit it to leave it as it is.
    post: Option<InfinityMode>,
}

fn parse_set_infinity(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetInfinityArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetInfinity {
            nodes: node_ids(args.node_ids)?,
            channels: channel_mask(args.channels)?,
            pre: args.pre.map(Infinity::from),
            post: args.post.map(Infinity::from),
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
        idempotent: false,
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
        name: "euler_filter",
        description: "Remove whole-turn jumps from rotation keys as one undo step, without changing the orientation at any key: each key is moved by whole turns per axis to lie nearest the previous key (half a turn is 180 degrees, or pi radians in a radian scene). When all three rotation channels are named and the object's three curves share their key frames, a key may instead take the equivalent triple (a + 180, 180 - b, c + 180), b being the middle axis of its rotate order, if that is nearer, which removes the flip a camera pan past 90 degrees produces. No node_ids means the object selection, and no channels means rotate_x, rotate_y and rotate_z. A non-rotation channel is refused with not_rotation_channel, named channels holding no key with no_rotation_keys, and a key beyond a million turns with euler_value_too_large; a refusal changes nothing. Filtering again changes nothing. With fewer than three channels or curves that do not share frames, each curve is only unrolled. The nearest-key rule, the triple form and the tie rule are PROVISIONAL: Maya's algorithm is unstated. Remaining keys are evaluated at the current time. Stops playback",
        schema: schema::<EulerFilterArgs>,
        parse: parse_euler_filter,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_infinity",
        description: "Set what keyed curves do before their first key (pre) and after their last (post), as one undo step: constant holds the end value, linear continues the end segment's slope, cycle repeats the keyed span, cycle_relative repeats it with the span's rise added each cycle, oscillate repeats it alternately forwards and backwards. At least one of pre and post is required, and one left out stays as it is. No node_ids means the object selection, and no channels means all nine; a named channel without a key is skipped, and none keyed is refused with no_keys. A refusal changes nothing. Read the settings as pre_infinity and post_infinity on each inspect_keys key. The setting lives with the curve's keys: deleting its last key resets it to constant, and it is session-only like the keys. Remaining keys are evaluated at the current time (PROVISIONAL: the modes' formulas are not stated by Maya's docs, and linear uses the end segment's slope because tangents are not implemented). Stops playback",
        schema: schema::<SetInfinityArgs>,
        parse: parse_set_infinity,
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
        description: "Start playback at 24 fps, looping continuously over the playback range, skipping frames rather than slowing down (rate and loop PROVISIONAL). A time outside the range starts at the range start. It advances only while the native window renders. It stops on a manipulation, a focused text field, a draft, a file modal or close. A request that passes the scene token and busy checks stops it before it runs, even when a later check refuses the request; a request refused by those checks, such as a stale scene_token, leaves it playing. The exceptions are requests that only select (objects, by rule or components, or the selection mode), set the active tool, its space or handle or the manipulator size, frame the view, orbit, pan, dolly or set the camera pose, set the grid, the shading mode, a sidebar panel or the Attribute Editor tab, read with inspect_scene, inspect_keys, inspect_uvs or get_recovery_status, or write a recovery file with autosave_now, and play_timeline, which leaves playback running (a repeated call is unchanged), and stop_timeline, which stops it itself. Read `timeline.playing`. Each frame that moves a keyed channel changes the revision, so paged cursors go stale during playback",
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
