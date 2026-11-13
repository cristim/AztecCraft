use crate::{
    editor_operations::{MAX_MOVED_VERTICES, Operation, PageCursor, RemoteRequest},
    mcp::{
        args::{decode, native_id, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::VertexSetTarget;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MoveVerticesArgs {
    scene_token: String,
    node_id: String,
    shape_id: String,
    #[schemars(length(min = 2, max = 257))]
    instance_path: Vec<String>,
    #[schemars(length(min = 1, max = MAX_MOVED_VERTICES))]
    vertex_indices: Vec<usize>,
    /// Offset along the mesh's own X, Y and Z axes in declared scene units.
    delta: [f64; 3],
}

fn parse_move_vertices(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: MoveVerticesArgs = decode(arguments)?;
    if !(2..=257).contains(&args.instance_path.len())
        || !(1..=MAX_MOVED_VERTICES).contains(&args.vertex_indices.len())
        || args.delta.iter().any(|v| !v.is_finite())
    {
        return Err(ErrorData::invalid_params(
            format!(
                "Moving vertices requires 2 to 257 path IDs, 1 to {MAX_MOVED_VERTICES} vertex indices and a finite delta"
            ),
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::MoveVertices {
            target: VertexSetTarget {
                transform: node_id(&args.node_id)?,
                shape: native_id(&args.shape_id)?,
                path: args
                    .instance_path
                    .iter()
                    .map(|value| native_id(value))
                    .collect::<Result<_, _>>()?,
                vertex_indices: args.vertex_indices,
            },
            delta: args.delta,
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectVerticesArgs {
    shape_id: String,
    /// The `next_cursor` of the previous reply. It is opaque and valid only until the scene
    /// changes.
    cursor: Option<String>,
}

fn parse_inspect_vertices(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: InspectVerticesArgs = decode(arguments)?;
    let cursor = args
        .cursor
        .map(|text| {
            text.parse::<PageCursor>()
                .map_err(|_| ErrorData::invalid_params("cursor is malformed", None))
        })
        .transpose()?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::InspectVertices {
            shape: native_id(&args.shape_id)?,
            cursor,
        },
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "move_vertices",
        description: "Translate vertices of one polygon mesh together by delta, in declared scene units along the mesh's own axes (the Move tool's Object axis orientation). Requires node, shape and full root-to-shape path and distinct vertex indices; shared-shape edits affect every instance. One undo step. Authored scenes only: scenes opened from a file refuse until their writer can carry edited points. World-axis moves are unsupported",
        schema: schema::<MoveVerticesArgs>,
        parse: parse_move_vertices,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "inspect_vertices",
        description: "Read one shape's vertex positions, up to 1024 per call, as [x, y, z] in declared scene units along the mesh's own axes (f32 precision for single-precision meshes). Reports the scene token and revision of the state read. A large shape is read in pages: follow `next_cursor` until it is null; it goes stale (`stale_scene`) as soon as the scene changes",
        schema: schema::<InspectVerticesArgs>,
        parse: parse_inspect_vertices,
        idempotent: false,
        read_only: true,
    },
];
