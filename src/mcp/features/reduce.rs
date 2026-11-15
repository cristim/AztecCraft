use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, native_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::ReduceTarget;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReduceMeshArgs {
    scene_token: String,
    shape_id: String,
    /// Remove this percentage of the triangles, above 0 and below 100.
    percent: Option<f64>,
    /// Stop at or below this many triangles.
    #[schemars(range(min = 1, max = 4294967295u32))]
    target_triangles: Option<u32>,
    /// Stop at this many vertices.
    #[schemars(range(min = 3, max = 4294967295u32))]
    target_vertices: Option<u32>,
}

fn parse_reduce_mesh(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ReduceMeshArgs = decode(arguments)?;
    let target = match (args.percent, args.target_triangles, args.target_vertices) {
        (Some(percent), None, None) => ReduceTarget::Percent(percent),
        (None, Some(count), None) => ReduceTarget::Triangles(count as usize),
        (None, None, Some(count)) => ReduceTarget::Vertices(count as usize),
        _ => {
            return Err(ErrorData::invalid_params(
                "reduce_mesh needs exactly one of percent, target_triangles and target_vertices",
                None,
            ));
        }
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ReduceMesh {
            shape: native_id(&args.shape_id)?,
            target,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "reduce_mesh",
    description: "Reduce one shape's polygon mesh as one undo step by edge-collapse decimation (PROVISIONAL: the polyReduce page states no algorithm): give exactly one of percent (share of triangles removed, above 0 and below 100), target_triangles (stop at or below) or target_vertices (stop at). Every face is triangulated; border vertices never move, no triangle flips or degenerates and the mesh stays manifold. A target the collapses cannot reach is refused with reduce_target_unreachable, a target that removes nothing with nothing_to_reduce, a bad value with invalid_reduce_target, a non-manifold mesh with reduce_not_manifold, vertices or edges no face uses with reduce_loose_geometry and a polygon that cannot be split with reduce_not_triangulable. A shape with UVs is refused with uvs_block_topology_edit (delete the UVs first); sourced scenes and holes are refused with unsupported_edit. A refusal changes nothing. Face, edge and vertex picks are cleared. Shared-shape edits affect every instance",
    schema: schema::<ReduceMeshArgs>,
    parse: parse_reduce_mesh,
    idempotent: false,
    read_only: false,
}];
