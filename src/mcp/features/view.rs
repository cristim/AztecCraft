use crate::{
    editor_operations::{Operation, RemoteRequest, ShadingMode},
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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OrbitArgs {
    scene_token: String,
    radians: [f32; 2],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PanArgs {
    scene_token: String,
    world_units: [f32; 2],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DollyArgs {
    scene_token: String,
    factor: f32,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GridArgs {
    scene_token: String,
    visible: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ShadingArgs {
    scene_token: String,
    mode: ShadingMode,
}

fn parse_frame_selected(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::FrameSelected)
}

fn parse_frame_all(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::FrameAll)
}

fn parse_set_grid_visibility(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: GridArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetGridVisible {
            visible: args.visible,
        },
    ))
}

fn parse_set_shading_mode(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ShadingArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetShadingMode(args.mode),
    ))
}

fn parse_orbit_camera(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: OrbitArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Orbit {
            radians: args.radians,
        },
    ))
}

fn parse_pan_camera(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: PanArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Pan {
            world_units: args.world_units,
        },
    ))
}

fn parse_dolly_camera(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: DollyArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Dolly {
            factor: args.factor,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "frame_selected",
        description: "Frame the selected object using the live camera",
        schema: schema::<MutationArgs>,
        parse: parse_frame_selected,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "frame_all",
        description: "Frame every mesh and curve in the scene using the live camera; an empty scene returns nothing_to_frame. Session-only; does not edit the scene or the selection",
        schema: schema::<MutationArgs>,
        parse: parse_frame_all,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "set_grid_visibility",
        description: "Set the current viewport's grid visibility explicitly. Session-only and idempotent; does not edit the scene or report GPU health",
        schema: schema::<GridArgs>,
        parse: parse_set_grid_visibility,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_shading_mode",
        description: "Set the current viewport's shading mode explicitly to wireframe or smooth_shade_all. Session-only and idempotent; does not edit the scene, change picking or report GPU health",
        schema: schema::<ShadingArgs>,
        parse: parse_set_shading_mode,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "orbit_camera",
        description: "Orbit the live camera by [yaw, pitch] radians; pitch clamps near the poles",
        schema: schema::<OrbitArgs>,
        parse: parse_orbit_camera,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "pan_camera",
        description: "Translate the live camera target by [right, up] camera-plane world units",
        schema: schema::<PanArgs>,
        parse: parse_pan_camera,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "dolly_camera",
        description: "Multiply live camera distance by a positive finite factor; below one moves closer",
        schema: schema::<DollyArgs>,
        parse: parse_dolly_camera,
        idempotent: false,
        read_only: false,
    },
];
