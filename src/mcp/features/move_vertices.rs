use crate::{
    editor_operations::{MAX_VERTICES_PER_PAGE, Operation, RemoteRequest},
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
    #[schemars(length(min = 1, max = 65536))]
    vertex_indices: Vec<usize>,
    /// Offset along the mesh's own X, Y and Z axes in declared scene units.
    delta: [f64; 3],
}

fn parse_move_vertices(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: MoveVerticesArgs = decode(arguments)?;
    if !(2..=257).contains(&args.instance_path.len())
        || !(1..=65536).contains(&args.vertex_indices.len())
        || args.delta.iter().any(|v| !v.is_finite())
    {
        return Err(ErrorData::invalid_params(
            "Moving vertices requires 2 to 257 path IDs, 1 to 65536 vertex indices and a finite delta",
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
    /// First vertex ordinal to read; default 0.
    offset: Option<usize>,
    /// Vertices to read, 1 to 1024; default 1024.
    #[schemars(range(min = 1, max = 1024))]
    count: Option<usize>,
}

fn parse_inspect_vertices(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: InspectVerticesArgs = decode(arguments)?;
    let count = args.count.unwrap_or(MAX_VERTICES_PER_PAGE);
    if !(1..=MAX_VERTICES_PER_PAGE).contains(&count) {
        return Err(ErrorData::invalid_params(
            "count must be 1 to 1024 vertices",
            None,
        ));
    }
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::InspectVertices {
            shape: native_id(&args.shape_id)?,
            offset: args.offset.unwrap_or(0),
            count,
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
        description: "Read one shape's vertex positions from offset, up to 1024 per call, as [x, y, z] in declared scene units along the mesh's own axes (f32 precision for single-precision meshes). Reports the scene token and revision of the state read; read the next window with a larger offset until offset plus the returned length reaches vertices_total",
        schema: schema::<InspectVerticesArgs>,
        parse: parse_inspect_vertices,
        idempotent: false,
        read_only: true,
    },
];
