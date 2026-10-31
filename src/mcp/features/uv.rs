use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, native_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::UvProjectAxis;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ProjectFrom {
    X,
    Y,
    Z,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProjectUvPlanarArgs {
    scene_token: String,
    shape_id: String,
    /// Project from this object-space axis. Omit it for Y (PROVISIONAL).
    axis: Option<ProjectFrom>,
}

fn parse_project_uv_planar(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ProjectUvPlanarArgs = decode(arguments)?;
    let axis = match args.axis {
        None => UvProjectAxis::default(),
        Some(ProjectFrom::X) => UvProjectAxis::X,
        Some(ProjectFrom::Y) => UvProjectAxis::Y,
        Some(ProjectFrom::Z) => UvProjectAxis::Z,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ProjectUvPlanar {
            shape: native_id(&args.shape_id)?,
            axis,
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DeleteUvsArgs {
    scene_token: String,
    shape_id: String,
}

fn parse_delete_uvs(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: DeleteUvsArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::DeleteUvs {
            shape: native_id(&args.shape_id)?,
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectUvsArgs {
    shape_id: String,
    /// The `next_cursor` of the previous reply. It is opaque and valid only until the scene
    /// changes.
    cursor: Option<String>,
}

fn parse_inspect_uvs(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: InspectUvsArgs = decode(arguments)?;
    let cursor = args
        .cursor
        .map(|text| {
            text.parse()
                .map_err(|_| ErrorData::invalid_params("cursor is malformed", None))
        })
        .transpose()?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::InspectUvs {
            shape: native_id(&args.shape_id)?,
            cursor,
        },
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "project_uv_planar",
        description: "Replace a shape's session-only map1 with a planar projection from the object-space X, Y or Z axis, fitted to the shape's bounding box so u and v fill 0 through 1. The axis-to-(u, v) assignment and the Y default are PROVISIONAL. Authored shapes only. UVs are not saved: Save refuses as for any new scene, and the authored writers refuse them too. Extrude is refused on a shape with UVs. A shape with no extent across the projection is refused with degenerate_uv_projection",
        schema: schema::<ProjectUvPlanarArgs>,
        parse: parse_project_uv_planar,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "delete_uvs",
        description: "Remove a shape's session-only map1. A shape without UVs returns unchanged",
        schema: schema::<DeleteUvsArgs>,
        parse: parse_delete_uvs,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "inspect_uvs",
        description: "Read one shape's session-only map1: one [u, v] per outer corner of each face, in face order. `uv_set` is null when the shape has no UVs. On a sourced scene `unavailable_reason` says source UVs are not exposed, which does not mean the shape has none. A large set is read in pages: follow `next_cursor` until it is null; it goes stale (`stale_scene`) as soon as the scene changes",
        schema: schema::<InspectUvsArgs>,
        parse: parse_inspect_uvs,
        idempotent: false,
        read_only: true,
    },
];
