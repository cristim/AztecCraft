use super::*;
use crate::editor_operations::Operation;
use azteccraft::primitive_options::SINGLE_QUAD_PLANE;

#[tokio::test]
async fn extrusion_moves_the_requested_cap_and_preserves_history_selection_and_other_nodes() {
    for distance in [1.25, 0.0, -0.75] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube","name":"extruded"}),
        )
        .await
        .structured_content
        .unwrap();
        let id = created["node_id"].as_str().unwrap();
        call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"plane","name":"unrelated"}),
        )
        .await;
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(inspected["inspection"]["units"]["linear"], "cm");
        let node = inspected["inspection"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == id)
            .unwrap();
        assert_eq!(node["shape_ids"].as_array().unwrap().len(), 1);
        assert_eq!(node["instance_paths"].as_array().unwrap().len(), 1);
        let shape = node["shape_ids"][0].as_str().unwrap();
        let mut path = node["instance_paths"][0].as_array().unwrap().clone();
        path.push(json!(shape));
        let shape_id = NativeId::from_decimal(shape).unwrap();
        let before = editor.history.scene().clone();
        let original_mesh = before.shape(shape_id).unwrap().mesh();
        let selection = (editor.selected(), editor.selected_path().to_vec());
        let camera = editor.camera.clone();
        let title = editor.document_title();
        let args = json!({"scene_token":token,"node_id":id,"shape_id":shape,"instance_path":path,"face_index":1,"distance":distance});
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "extrude_face",
            args.clone(),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
        let extruded = editor.history.scene().clone();
        let mesh = extruded.shape(shape_id).unwrap().mesh();
        assert_eq!(
            (mesh.point_count(), mesh.edges().len(), mesh.faces().len()),
            (12, 20, 10)
        );
        assert_eq!(&mesh.edges()[..12], original_mesh.edges());
        for index in 0..8 {
            assert_eq!(
                mesh.effective_point_cm(index).unwrap(),
                original_mesh.effective_point_cm(index).unwrap()
            );
        }
        for index in 0..4 {
            let mut expected = original_mesh.effective_point_cm(index + 4).unwrap();
            expected[2] += distance;
            assert_eq!(mesh.effective_point_cm(index + 8).unwrap(), expected);
        }
        for index in [0, 2, 3, 4, 5] {
            assert_eq!(mesh.faces()[index], original_mesh.faces()[index]);
        }
        let cap: Vec<_> = mesh.faces()[1]
            .outer()
            .iter()
            .map(|edge| mesh.edges()[edge.edge].endpoints()[usize::from(edge.reversed)])
            .collect();
        assert_eq!(cap, [8, 9, 10, 11]);
        assert!(
            mesh.faces()[6..]
                .iter()
                .all(|face| face.outer().len() == 4 && face.holes().is_empty())
        );
        let unrelated = selection.0.unwrap();
        assert_eq!(
            extruded.node(unrelated).unwrap(),
            before.node(unrelated).unwrap()
        );
        let unrelated_shape = before
            .shapes()
            .iter()
            .find(|shape| shape.id() != shape_id)
            .unwrap();
        assert_eq!(
            extruded.shape(unrelated_shape.id()).unwrap(),
            unrelated_shape
        );
        assert_eq!(
            (editor.selected(), editor.selected_path()),
            (selection.0, selection.1.as_slice())
        );
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.document_title(), title);
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        let mut obsolete_side = args;
        obsolete_side["face_index"] = json!(6);
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "extrude_face",
            obsolete_side,
        )
        .await;
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &extruded);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn extrusion_refuses_mismatched_current_targets_planes_and_imported_structure() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    let shape = editor.history.scene().shapes()[0].id();
    let transform = editor.history.scene().node(id).unwrap().transform_id();
    // A single-quad plane, so face 0 is every face and all-face extrusion is refused.
    editor.native_operation(Operation::Create {
        primitive: Primitive::Plane,
        options: Some(SINGLE_QUAD_PLANE),
        name: None,
    });
    let other = editor.selected().unwrap();
    let other_shape = editor.history.scene().shapes()[1].id();
    let other_transform = editor.history.scene().node(other).unwrap().transform_id();
    let before = editor.history.scene().clone();
    let selection = (editor.selected(), editor.selected_path().to_vec());
    let camera = editor.camera.clone();
    let fields = (editor.name.clone(), editor.channels.clone());
    let valid = json!({"scene_token":token,"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[transform.to_decimal(),shape.to_decimal()],"face_index":1,"distance":1});
    for delta in [
        json!({"node_id":other.to_decimal()}),
        json!({"node_id":"999999"}),
        json!({"shape_id":other_shape.to_decimal()}),
        json!({"shape_id":"999999"}),
        json!({"instance_path":[other_transform.to_decimal(),shape.to_decimal()]}),
        json!({"instance_path":[transform.to_decimal(),transform.to_decimal(),shape.to_decimal()]}),
        json!({"instance_path":[transform.to_decimal(),other_shape.to_decimal()]}),
        json!({"face_index":usize::MAX}),
        json!({"distance":f64::MAX}),
        json!({"node_id":other.to_decimal(),"shape_id":other_shape.to_decimal(),"instance_path":[other_transform.to_decimal(),other_shape.to_decimal()],"face_index":0}),
    ] {
        let mut args = valid.clone();
        args.as_object_mut()
            .unwrap()
            .extend(delta.as_object().unwrap().clone());
        let result = call(&mut editor, &context, client.peer(), "extrude_face", args).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(
            (editor.selected(), editor.selected_path()),
            (selection.0, selection.1.as_slice())
        );
        assert_eq!(editor.camera, camera);
        assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
        assert!(!editor.history.can_redo());
    }
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("imported.mb");
    let source = binary_fixture::specimen();
    std::fs::write(&file, &source).unwrap();
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":file.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    let opened = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"discard"})).await.structured_content.unwrap();
    let token = opened["scene_token"].as_str().unwrap();
    let imported = editor.history.scene().clone();
    let id = imported.nodes()[0].id();
    let shape = imported.shapes()[0].id();
    let result = call(&mut editor, &context, client.peer(), "extrude_face", json!({"scene_token":token,"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[imported.node(id).unwrap().transform_id().to_decimal(),shape.to_decimal()],"face_index":0,"distance":1})).await;
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &imported);
    assert!(!editor.history.can_undo());
    assert!(!editor.history.can_redo());
    assert!(!editor.is_dirty());
    assert!(std::fs::read(file).unwrap() == source);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
