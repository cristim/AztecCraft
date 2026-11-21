use crate::{
    editor_operations::{Operation, PlayblastFormat, PlayblastRequest, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, schema, scoped, token},
        registry::ToolSpec,
    },
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Format {
    Png,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PlayblastArgs {
    scene_token: String,
    /// First frame, a whole number from 0; defaults to the playback range start.
    start_frame: Option<i64>,
    /// Last frame, inclusive; defaults to the playback range end.
    end_frame: Option<i64>,
    /// Width in pixels at 100 percent, 1 to 8192. Required: no viewport size is stored.
    #[schemars(range(min = 1, max = 8192))]
    width: u32,
    /// Height in pixels at 100 percent, 1 to 8192.
    #[schemars(range(min = 1, max = 8192))]
    height: u32,
    /// Output size as a percent of width and height, 1 to 100; default 100.
    #[schemars(range(min = 1, max = 100))]
    percent: Option<u32>,
    /// Absolute path of an existing directory that receives the frames.
    #[schemars(length(min = 1, max = 4096))]
    output_directory: String,
    /// Frame file stem, 1 to 64 of A-Z a-z 0-9 _ -; default "playblast".
    name: Option<String>,
    /// Image format; only png, a numbered image sequence.
    format: Option<Format>,
    /// Draw the grid and selection feedback; default true.
    show_ornaments: Option<bool>,
}

fn parse_playblast(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: PlayblastArgs = decode(arguments)?;
    let directory = Path::new(&args.output_directory);
    if args.output_directory.is_empty()
        || args.output_directory.len() > 4096
        || args.output_directory.contains('\0')
        || !directory.is_absolute()
    {
        return Err(ErrorData::invalid_params(
            "output_directory must be an absolute path of 1 to 4096 UTF-8 bytes and no NUL",
            None,
        ));
    }
    let percent = args.percent.unwrap_or(100);
    if !(1..=100).contains(&percent) {
        return Err(ErrorData::invalid_params("percent must be 1 to 100", None));
    }
    let Format::Png = args.format.unwrap_or(Format::Png);
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Playblast(PlayblastRequest {
            directory: directory.to_path_buf(),
            name: args.name.unwrap_or_else(|| "playblast".into()),
            start: args.start_frame.map(|frame| frame as f64),
            end: args.end_frame.map(|frame| frame as f64),
            width: Some(args.width),
            height: Some(args.height),
            percent,
            format: PlayblastFormat::Png,
            offscreen: true,
            show_ornaments: args.show_ornaments.unwrap_or(true),
        }),
    ))
}

fn parse_cancel_playblast(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::CancelPlayblast)
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "playblast",
        description: "Start a playblast: render the viewport camera over start_frame..end_frame (inclusive) offscreen into a numbered PNG sequence <name>.0001.png in the existing absolute output_directory, as a background job on a snapshot of the scene (the scene, selection, history and current time are never changed). width and height (the 100% size) are required; percent scales them (1 to 100, default 100); show_ornaments (default true) keeps the grid and selection feedback. Replies with playblast_started and the frame count and output size; poll inspect_scene playblast (state, frames_done, frames_total, directory_name, last_error) for progress, and call cancel_playblast to stop. Never overwrites: any existing target file is refused with destination_exists before frame 1. Refused with playblast_busy while a job runs, busy_editing during a draft, modal or close, playblast_invalid, playblast_too_large (at most 100000 frames, 8192 per side, 16 megapixels a frame, 4 GiB in total), playblast_no_adapter without a GPU device, playblast_io or playblast_render on failure; frames written before a failure stay. A modal or draft pauses the job. Maya playblast -startTime and -endTime default to the playback range, -percent, -format image, -offScreen and -showOrnaments are the nearest flags. PROVISIONAL: Maya's page does not say whether -endTime is inclusive, its percent is 10 to 100 (clamped, default 50) and its default format is a movie; naming, caps, the ornaments mapping and the viewport camera are ours. No movie container, no image plane and no wireframe on shaded yet",
        schema: schema::<PlayblastArgs>,
        parse: parse_playblast,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "cancel_playblast",
        description: "Stop the running playblast after the file in flight, keep the frames already written and join its writer; replies changed, or unchanged when no job runs. inspect_scene playblast then reports state cancelled with frames_done",
        schema: schema::<MutationArgs>,
        parse: parse_cancel_playblast,
        idempotent: false,
        read_only: false,
    },
];
