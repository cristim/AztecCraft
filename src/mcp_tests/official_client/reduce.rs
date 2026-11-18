use super::*;

fn error_code(result: &rmcp::model::CallToolResult) -> &str {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &'static str,
    mut arguments: Value,
) -> rmcp::model::CallToolResult {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments).await
}

fn face_count(editor: &Editor, shape: NativeId) -> usize {
    editor
        .history
        .scene()
        .shape(shape)
        .unwrap()
        .mesh()
        .faces()
        .len()
}

#[tokio::test]
async fn reduce_mesh_is_one_undo_step_and_triangulates() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let shape = editor.history.scene().shapes()[0].id();
    let before = editor.history.scene().clone();

    let args = json!({"shape_id": shape.to_decimal(), "percent": 50});
    let reduced = tool(&mut editor, &context, &client, "reduce_mesh", args).await;
    assert_eq!(reduced.structured_content.unwrap()["outcome"], "changed");
    let after = editor.history.scene().clone();
    let mesh = after.shape(shape).unwrap().mesh();
    assert!(mesh.faces().len() < face_count_of(&before, shape));
    assert!(mesh.faces().iter().all(|f| f.outer().len() == 3));

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert!(*editor.history.scene() == before);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert!(*editor.history.scene() == after);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

fn face_count_of(scene: &azteccraft::native_scene::NativeScene, shape: NativeId) -> usize {
    scene.shape(shape).unwrap().mesh().faces().len()
}

#[tokio::test]
async fn reduce_mesh_counts_reach_their_target() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();

    tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "target_vertices": 60}),
    )
    .await;
    let mesh = editor.history.scene().shape(shape).unwrap().mesh().clone();
    assert_eq!(mesh.point_count(), 60);
    tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "target_triangles": 50}),
    )
    .await;
    assert!(face_count(&editor, shape) <= 50);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_refusals_keep_the_scene_and_history() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Plane);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();
    let before = editor.history.scene().clone();

    for (args, code) in [
        (
            json!({"shape_id": id, "percent": 0}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "percent": 100}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "target_vertices": 2}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "target_triangles": 0}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "target_triangles": 100000}),
            "nothing_to_reduce",
        ),
        (
            json!({"shape_id": id, "target_triangles": 1}),
            "reduce_target_unreachable",
        ),
        (json!({"shape_id": "999999", "percent": 50}), "scene_error"),
    ] {
        let refused = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
        assert_eq!(refused.is_error, Some(true), "{args}");
        assert_eq!(error_code(&refused), code, "{args}");
        assert!(*editor.history.scene() == before, "{args}");
    }

    for args in [
        json!({"shape_id": id}),
        json!({"shape_id": id, "percent": 50, "target_vertices": 10}),
        json!({"shape_id": id, "percent": "half"}),
        json!({"shape_id": id, "percent": 50, "extra": 1}),
        json!({"shape_id": id, "target_triangles": -3}),
    ] {
        let mut args = args;
        args["scene_token"] = json!(editor.scene_token.to_string());
        let malformed =
            call_response(&mut editor, &context, client.peer(), "reduce_mesh", args).await;
        assert!(malformed.is_err());
        assert!(*editor.history.scene() == before);
    }

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "reduce_mesh",
        json!({"scene_token": Uuid::new_v4().to_string(), "shape_id": id, "percent": 50}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_is_refused_while_a_shape_has_uvs() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let id = editor.history.scene().shapes()[0].id().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        json!({"shape_id": id}),
    )
    .await;
    let before = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "percent": 50}),
    )
    .await;
    assert_eq!(error_code(&refused), "uvs_block_topology_edit");
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_respects_drafts_and_modals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let id = editor.history.scene().shapes()[0].id().to_decimal();
    let before = editor.history.scene().clone();

    editor.channels[0] = "7".into();
    let args = json!({"shape_id": id, "percent": 50});
    let busy = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
    assert_eq!(error_code(&busy), "busy_editing");
    assert_eq!(editor.channels[0], "7");
    assert!(*editor.history.scene() == before);
    editor.sync_fields();

    editor
        .apply_operation(Operation::NewScene)
        .expect("dirty scene asks first");
    assert!(editor.files.is_modal());
    let busy = tool(&mut editor, &context, &client, "reduce_mesh", args).await;
    assert_eq!(error_code(&busy), "busy_editing");
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

/// Face indices of the central part of the default plane (centroid within half the extent), and the
/// number of vertices the region touches.
fn middle_faces(editor: &Editor, shape: NativeId) -> (Vec<usize>, usize) {
    let scene = editor.history.scene();
    let mesh = scene.shape(shape).unwrap().mesh();
    let corners = |face: &azteccraft::native_scene::NativeFace| -> Vec<usize> {
        face.outer()
            .iter()
            .map(|d| mesh.edges()[d.edge].endpoints()[usize::from(d.reversed)])
            .collect()
    };
    let at = |v: usize| mesh.effective_point_cm(v).unwrap();
    let half = (0..mesh.point_count())
        .map(|v| at(v)[0].abs().max(at(v)[2].abs()))
        .fold(0.0, f64::max);
    let mut region = Vec::new();
    let mut vertices = std::collections::BTreeSet::new();
    for (index, face) in mesh.faces().iter().enumerate() {
        let c = corners(face);
        let n = c.len() as f64;
        let (x, z) = c.iter().fold((0.0, 0.0), |s, v| {
            (s.0 + at(*v)[0] / n, s.1 + at(*v)[2] / n)
        });
        if x.abs() <= half / 2.0 && z.abs() <= half / 2.0 {
            region.push(index);
            vertices.extend(c);
        }
    }
    (region, vertices.len())
}

#[tokio::test]
async fn reduce_mesh_region_reduces_only_the_faces_and_undoes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Plane);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();
    let (region, region_vertices) = middle_faces(&editor, shape);
    assert!(region.len() >= 4);
    let before = editor.history.scene().clone();
    let outside = face_count(&editor, shape) - region.len();

    // A remembered face pick must not survive the renumbering.
    let mut path = editor.selected_path().to_vec();
    path.push(shape);
    editor.set_remembered_face(Some(azteccraft::native_scene::FaceTarget {
        transform: editor.selected().unwrap(),
        shape,
        path,
        face_index: region[0],
    }));
    assert!(
        !editor
            .component_set(crate::editor_operations::ComponentKind::Face)
            .is_empty()
    );
    let reduced = tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "percent": 30, "faces": region}),
    )
    .await;
    assert_eq!(reduced.structured_content.unwrap()["outcome"], "changed");
    let after = editor.history.scene().clone();
    let mesh = after.shape(shape).unwrap().mesh();
    let quads = mesh.faces().iter().filter(|f| f.outer().len() == 4).count();
    assert_eq!(
        quads, outside,
        "faces outside the region keep their polygons"
    );
    let removed = before.shape(shape).unwrap().mesh().point_count() - mesh.point_count();
    assert_eq!(removed, region_vertices * 30 / 100);
    assert!(
        editor
            .component_set(crate::editor_operations::ComponentKind::Face)
            .is_empty()
    );

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert!(*editor.history.scene() == before);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert!(*editor.history.scene() == after);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_region_refusals_keep_the_scene_and_history() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Plane);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();
    let total = face_count(&editor, shape);
    let before = editor.history.scene().clone();

    for (args, code) in [
        (
            json!({"shape_id": id, "percent": 30, "faces": []}),
            "reduce_region_empty",
        ),
        (
            json!({"shape_id": id, "percent": 30, "faces": [0, total]}),
            "reduce_face_out_of_range",
        ),
        (
            json!({"shape_id": id, "percent": 30, "faces": [2, 5, 2]}),
            "reduce_duplicate_face",
        ),
        // One quad touches nothing it may collapse.
        (
            json!({"shape_id": id, "percent": 30, "faces": [0]}),
            "reduce_target_unreachable",
        ),
        (
            json!({"shape_id": id, "target_triangles": 1, "faces": [0, 1]}),
            "reduce_target_unreachable",
        ),
        (
            json!({"shape_id": id, "percent": 0, "faces": [0, 1]}),
            "invalid_reduce_target",
        ),
    ] {
        let refused = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
        assert_eq!(refused.is_error, Some(true), "{args}");
        assert_eq!(error_code(&refused), code, "{args}");
        assert!(*editor.history.scene() == before, "{args}");
    }

    for faces in [json!("all"), json!([-1]), json!([1.5]), json!([null])] {
        let mut args = json!({"shape_id": id, "percent": 30, "faces": faces});
        args["scene_token"] = json!(editor.scene_token.to_string());
        let malformed =
            call_response(&mut editor, &context, client.peer(), "reduce_mesh", args).await;
        assert!(malformed.is_err());
        assert!(*editor.history.scene() == before);
    }

    // At the cap the list reaches the core (a repeated index: reduce_duplicate_face); one more is
    // refused by the schema or the tool before the core sees it. Both fit the ingress line. The
    // cap is the tool's MAX_REDUCE_FACES, pinned against the schema by a registry unit test.
    let cap = 8192;
    let at_cap = tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "percent": 30, "faces": vec![0; cap]}),
    )
    .await;
    assert_eq!(error_code(&at_cap), "reduce_duplicate_face");
    let mut args = json!({"shape_id": id, "percent": 30, "faces": vec![0; cap + 1]});
    args["scene_token"] = json!(editor.scene_token.to_string());
    let over = call_response(&mut editor, &context, client.peer(), "reduce_mesh", args).await;
    assert!(over.is_err());
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_region_respects_drafts_and_modals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Plane);
    let shape = editor.history.scene().shapes()[0].id();
    let (region, _) = middle_faces(&editor, shape);
    let args = json!({"shape_id": shape.to_decimal(), "percent": 30, "faces": region});
    let before = editor.history.scene().clone();

    editor.channels[0] = "7".into();
    let busy = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
    assert_eq!(error_code(&busy), "busy_editing");
    assert_eq!(editor.channels[0], "7");
    assert!(*editor.history.scene() == before);
    editor.sync_fields();

    editor
        .apply_operation(Operation::NewScene)
        .expect("dirty scene asks first");
    let busy = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
    assert_eq!(error_code(&busy), "busy_editing");
    assert!(*editor.history.scene() == before);

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "reduce_mesh",
        json!({"scene_token": Uuid::new_v4().to_string(), "shape_id": shape.to_decimal(), "percent": 30, "faces": [0, 1]}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");

    client.cancel().await.unwrap();
    server.await.unwrap();
}
