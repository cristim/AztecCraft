use crate::{
    editor_operations::{Operation, RemoteRequest},
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

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "move_vertices",
    description: "Translate vertices of one polygon mesh together by delta, in declared scene units along the mesh's own axes (the Move tool's Object axis orientation). Requires node, shape and full root-to-shape path and distinct vertex indices; shared-shape edits affect every instance. One undo step. Authored scenes only: scenes opened from a file refuse until their writer can carry edited points. World-axis moves are unsupported",
    schema: schema::<MoveVerticesArgs>,
    parse: parse_move_vertices,
    idempotent: false,
    read_only: false,
}];
