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

/// Most face indices one request lists: six-digit indices (a mesh has at most 200000 triangles)
/// plus separators must fit the transport's ingress line; a test checks the worst case.
pub(in crate::mcp) const MAX_REDUCE_FACES: usize = 8192;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReduceMeshArgs {
    scene_token: String,
    shape_id: String,
    /// Remove this percentage of the original vertices, above 0 and below 100; the count removed is
    /// rounded down.
    percent: Option<f64>,
    /// Stop at or below this many triangles.
    #[schemars(range(min = 1, max = 4294967295u32))]
    target_triangles: Option<u32>,
    /// Stop at this many vertices.
    #[schemars(range(min = 3, max = 4294967295u32))]
    target_vertices: Option<u32>,
    /// Reduce only these faces (0-based positions in the mesh's face list), at most 8192, no
    /// duplicates.
    /// Percent and counts then refer to the region's own vertices and triangles.
    #[schemars(length(max = 8192))]
    faces: Option<Vec<usize>>,
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
    if args
        .faces
        .as_ref()
        .is_some_and(|f| f.len() > MAX_REDUCE_FACES)
    {
        return Err(ErrorData::invalid_params(
            format!("reduce_mesh faces lists at most {MAX_REDUCE_FACES} indices"),
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ReduceMesh {
            shape: native_id(&args.shape_id)?,
            target,
            faces: args.faces,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "reduce_mesh",
    description: "Reduce one shape's polygon mesh as one undo step by edge-collapse decimation (PROVISIONAL: the polyReduce page states no collapse order, cost or tie-break): give exactly one of percent (share of the original vertices removed as polyReduce -percentage says, above 0 and below 100, rounded down so never more: 30 percent of 81 vertices keeps 57), target_triangles (stop at or below) or target_vertices (stop at). Every face is triangulated; border vertices never move, no triangle flips (its new normal must keep a positive dot with the old one, so a fold near 90 degrees can pass) or degenerates and the mesh stays manifold. A target the collapses cannot reach is refused with reduce_target_unreachable, a target that removes nothing with nothing_to_reduce, a bad value with invalid_reduce_target, a non-manifold mesh with reduce_not_manifold, vertices or edges no face uses with reduce_loose_geometry and a polygon that cannot be split with reduce_not_triangulable and more than 200000 triangles with reduce_too_large. With faces (at most 8192, 0-based positions in the mesh's face list), only those faces collapse: vertices on the region boundary stay put like mesh borders, faces outside the region keep their polygons and vertex positions (vertex indices shift past removed vertices) and percent, target_triangles and target_vertices count the region's own vertices (every vertex of a region face) and triangles. An empty faces list is refused with reduce_region_empty, an index past the last face with reduce_face_out_of_range, a repeated index with reduce_duplicate_face; a region that cannot reach the target is reduce_target_unreachable, reporting the region's best counts. A shape with UVs is refused with uvs_block_topology_edit (delete the UVs first); sourced scenes and holes are refused with unsupported_edit. A refusal changes nothing. Face, edge and vertex picks are cleared. Shared-shape edits affect every instance",
    schema: schema::<ReduceMeshArgs>,
    parse: parse_reduce_mesh,
    idempotent: false,
    read_only: false,
}];
