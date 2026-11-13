use crate::{
    editor_operations::{Operation, RemoteRequest, ShadingMode},
    mcp::{
        args::{MutationArgs, decode, mutation, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::viewport::MAX_PITCH;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FrameSelectedArgs {
    scene_token: String,
    /// Also frame the selected objects' descendants (Maya Ctrl+F). Default false (Maya F).
    #[serde(default)]
    with_children: bool,
}

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
struct SetCameraPoseArgs {
    scene_token: String,
    target: [f64; 3],
    distance: f64,
    yaw_degrees: f64,
    pitch_degrees: f64,
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
    let args: FrameSelectedArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::FrameSelected {
            with_children: args.with_children,
        },
    ))
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

/// Narrows after the f64 arithmetic so a pose read from `inspect_scene` sets back bitwise.
fn camera_f32(name: &str, value: f64) -> Result<f32, ErrorData> {
    let narrowed = value as f32;
    if narrowed.is_finite() {
        Ok(narrowed)
    } else {
        Err(ErrorData::invalid_params(
            format!("{name} must be a finite number within single-precision range"),
            None,
        ))
    }
}

fn parse_set_camera_pose(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetCameraPoseArgs = decode(arguments)?;
    let token = token(&args.scene_token)?;
    let target = [
        camera_f32("target", args.target[0])?,
        camera_f32("target", args.target[1])?,
        camera_f32("target", args.target[2])?,
    ];
    let distance = camera_f32("distance", args.distance)?;
    if distance <= 0.0 {
        return Err(ErrorData::invalid_params(
            "distance must be greater than zero",
            None,
        ));
    }
    let yaw = camera_f32("yaw_degrees", args.yaw_degrees.to_radians())?;
    let pitch = camera_f32("pitch_degrees", args.pitch_degrees.to_radians())?;
    if pitch.abs() > MAX_PITCH {
        return Err(ErrorData::invalid_params(
            format!(
                "pitch_degrees must be within \u{b1}{}",
                MAX_PITCH.to_degrees()
            ),
            None,
        ));
    }
    Ok(scoped(
        token,
        Operation::SetCameraPose {
            target,
            distance,
            yaw,
            pitch,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "frame_selected",
        description: "Frame the selected objects using the live camera (Maya F): the union of the selected objects' own geometry (meshes, curves and joints) without their descendants, or with them when with_children is true (Ctrl+F; default false). A selection where no object owns visible geometry frames the objects' origins when with_children is false; with_children true on such a selection is refused and the camera is unchanged. In face, vertex and edge modes it frames the selected face corners, the remembered vertex or both endpoints of the remembered edge and ignores with_children. Nothing selected in object or hierarchy mode frames everything like frame_all (an empty scene returns nothing_to_frame); in the component modes it returns no_selection",
        schema: schema::<FrameSelectedArgs>,
        parse: parse_frame_selected,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "frame_all",
        description: "Frame every mesh, curve and joint in the scene using the live camera; a scene with none of these returns nothing_to_frame. Session-only; does not edit the scene or the selection",
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
        name: "set_camera_pose",
        description: "Set the live camera to an absolute pose: target [x, y, z], distance, yaw_degrees and pitch_degrees, the values inspect_scene reports, so a read pose restores exactly. Yaw wraps; pitch beyond about \u{b1}89.94 degrees is rejected. The field of view and clip planes are untouched; a distance below near_clip puts the target closer to the eye than the near plane (between the eye and the plane), so it is clipped and not pickable. Session-only; does not edit the scene",
        schema: schema::<SetCameraPoseArgs>,
        parse: parse_set_camera_pose,
        idempotent: true,
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
