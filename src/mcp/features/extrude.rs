use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, native_id, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::FaceTarget;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExtrudeArgs {
    scene_token: String,
    node_id: String,
    shape_id: String,
    #[schemars(length(min = 2, max = 257))]
    instance_path: Vec<String>,
    face_index: usize,
    distance: f64,
}

fn parse_extrude_face(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ExtrudeArgs = decode(arguments)?;
    if !(2..=257).contains(&args.instance_path.len()) || !args.distance.is_finite() {
        return Err(ErrorData::invalid_params(
            "Extrusion requires 2 to 257 path IDs and a finite distance",
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ExtrudeFace {
            target: FaceTarget {
                transform: node_id(&args.node_id)?,
                shape: native_id(&args.shape_id)?,
                path: args
                    .instance_path
                    .iter()
                    .map(|value| native_id(value))
                    .collect::<Result<_, _>>()?,
                face_index: args.face_index,
            },
            distance: args.distance,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[ToolSpec {
    name: "extrude_face",
    description: "Extrude one current canonical quad along its local normal by distance in declared scene units. Requires node, shape and full root-to-shape path; shared-shape edits affect every instance. Zero distance changes topology; imported structural edits remain unsupported. A result whose world geometry leaves the f32 range the viewport draws is refused with world_out_of_range. All or nothing: the scene and undo stack are untouched on any refusal",
    schema: schema::<ExtrudeArgs>,
    parse: parse_extrude_face,
    idempotent: false,
    read_only: false,
}];
