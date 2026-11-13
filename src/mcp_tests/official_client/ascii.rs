use super::*;

async fn ascii_open_inspect_select_frame(path: &std::path::Path, expected_counts: [usize; 2]) {
    let source = std::fs::read(path).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let catalog = client.peer().list_tools(None).await.unwrap();
    assert_eq!(
        catalog
            .tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>(),
        crate::mcp::tool_names().collect::<Vec<_>>()
    );
    let old_token = editor.scene_token.to_string();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":old_token,"path":path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let token = editor.scene_token.to_string();
    assert_ne!(token, old_token);
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
    let inspection = &inspected["inspection"];
    assert_eq!(inspection["document_origin"], "maya_ascii");
    assert_eq!(inspection["retained_source_bytes"], source.len());
    assert!(inspection["retained_source_records"].as_u64().unwrap() > 0);
    assert!(!inspection["read_only_reason"].as_str().unwrap().is_empty());
    assert_eq!(inspection["shapes"].as_array().unwrap().len(), 0);
    let curves = inspection["curves"].as_array().unwrap();
    assert_eq!(curves.len(), 1);
    assert_eq!(curves[0]["control_vertices"], expected_counts[0]);
    assert_eq!(curves[0]["knots"], expected_counts[1]);
    assert_eq!(curves[0]["degree"], 3);
    assert_eq!(curves[0]["form"], "open");
    assert_eq!(curves[0]["read_only"], true);
    assert_eq!(curves[0]["draw_available"], true);
    assert_eq!(inspection["save_available"], false);
    assert_eq!(inspection["save_as_available"], false);
    assert!(
        !serde_json::to_string(inspection)
            .unwrap()
            .contains("inert scene script")
    );
    let owner = inspection["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| {
            node["shape_ids"]
                .as_array()
                .unwrap()
                .contains(&curves[0]["id"])
        })
        .unwrap();
    let id = owner["id"].as_str().unwrap().to_owned();
    let instance_path = owner["instance_paths"][0].clone();
    let selected = call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":id,"instance_path":instance_path}),
    )
    .await;
    assert_eq!(selected.is_error, Some(false));
    assert_eq!(editor.selected().unwrap().to_decimal(), id);
    let baseline = editor.history.scene().clone();
    let pan = call(
        &mut editor,
        &context,
        client.peer(),
        "pan_camera",
        json!({"scene_token":token,"world_units":[100,25]}),
    )
    .await;
    assert_eq!(pan.is_error, Some(false));
    let frame = call(
        &mut editor,
        &context,
        client.peer(),
        "frame_selected",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(frame.is_error, Some(false));
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    assert_eq!(view.curve_instances().len(), 1);
    for point in view.curve_instances()[0].world_segments().flatten() {
        let ndc = editor
            .camera
            .project(point, editor.aspect)
            .unwrap()
            .unwrap();
        assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
    }
    for (tool, fields) in [
        ("create_primitive", json!({"primitive":"cube"})),
        ("rename_node", json!({"node_id":id,"name":"must_not_apply"})),
        (
            "set_transform",
            json!({"node_id":id,"translation":[1,2,3],"rotation_degrees":[0,0,0],"scale":[1,1,1]}),
        ),
        (
            "edit_node",
            json!({"node_id":id,"translation":[1,null,null]}),
        ),
        ("delete_node", json!({"node_id":id})),
        ("delete_selected", json!({})),
    ] {
        let mut args = fields;
        args["scene_token"] = json!(token);
        let result = call(&mut editor, &context, client.peer(), tool, args)
            .await
            .structured_content
            .unwrap();
        assert_eq!(result["error"]["code"], "scene_error");
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("read-only")
        );
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(editor.selected().unwrap().to_decimal(), id);
        assert!(!editor.is_dirty() && !editor.history.can_undo());
    }
    let destination_directory = tempfile::tempdir().unwrap();
    let destination = destination_directory.path().join("must_not_write.mb");
    for (tool, fields) in [
        ("save_scene", json!({})),
        (
            "save_scene_as",
            json!({"path":destination.to_str().unwrap()}),
        ),
    ] {
        let mut args = fields;
        args["scene_token"] = json!(token);
        let result = call(&mut editor, &context, client.peer(), tool, args)
            .await
            .structured_content
            .unwrap();
        assert_eq!(result["error"]["code"], "save_unsupported");
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(editor.scene_token.to_string(), token);
        assert!(!editor.is_dirty() && !editor.history.can_undo());
        assert!(!destination.exists());
    }
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap(),"extra":true})
        )
        .await
        .is_err()
    );
    assert_eq!(editor.history.scene(), &baseline);
    assert_eq!(std::fs::read(path).unwrap(), source);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn ascii_sdk_open_inspect_select_frame_uses_canonical_readonly_curves() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    ascii_open_inspect_select_frame(&path, [4, 6]).await;
}

#[tokio::test]
#[ignore = "requires the explicitly supplied pinned genuine Maya ASCII fixture"]
async fn genuine_ascii_sdk_open_inspect_select_frame_uses_actual_editor() {
    let path = std::env::var_os("AZTECCRAFT_MA_CURVE_FIXTURE").unwrap();
    ascii_open_inspect_select_frame(std::path::Path::new(&path), [8, 10]).await;
}

#[tokio::test]
async fn ascii_sdk_rejected_open_preserves_dirty_nonempty_scene_history_and_selection() {
    let directory = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    let id = editor.selected().unwrap().to_decimal();
    call(
        &mut editor,
        &context,
        client.peer(),
        "rename_node",
        json!({"scene_token":token,"node_id":id,"name":"undone_name"}),
    )
    .await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert!(editor.history.can_undo() && editor.history.can_redo() && editor.is_dirty());
    let baseline = editor.history.scene().clone();
    let before = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    let camera = editor.camera.clone();
    let selected = (editor.selected(), editor.selected_path().to_vec());
    let valid = String::from_utf8(ascii_fixture::specimen()).unwrap();
    for (index, source) in [
        String::from("//Maya ASCII 2016 scene\ncreateNode"),
        valid.replace("createNode nurbsCurve", "createNode mesh"),
        valid.replace("12 0 0", "100000000000000000000 0 0"),
    ]
    .into_iter()
    .enumerate()
    {
        let path = directory.path().join(format!("rejected{index}.ma"));
        std::fs::write(&path, source.as_bytes()).unwrap();
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await;
        let error = result.structured_content.unwrap()["error"].clone();
        if index == 2 {
            assert_eq!(error["code"], "view_error");
            let message = error["message"].as_str().unwrap();
            assert!(!message.contains("withheld") && !message.contains(".ma"));
            assert!(message.contains("world geometry") || message.contains("view"));
        } else {
            assert_eq!(error["code"], "file_error");
        }
        let after = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(after, before);
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(editor.camera, camera);
        assert_eq!(
            (editor.selected(), editor.selected_path().to_vec()),
            selected
        );
        assert!(editor.history.can_undo() && editor.history.can_redo());
        assert_eq!(std::fs::read(path).unwrap(), source.as_bytes());
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn ncrv_sdk_open_refusal_preserves_the_complete_editor_baseline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.mb");
    let baseline = binary_fixture::specimen();
    std::fs::write(&path, &baseline).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let token = editor.scene_token.to_string();
    let id = editor.history.scene().nodes()[0].id();
    let selected = call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":id.to_decimal()}),
    )
    .await;
    assert!(selected.structured_content.unwrap().get("error").is_none());
    assert_eq!(editor.selected(), Some(id));
    assert!(!editor.selected_path().is_empty());
    let changed = call(
        &mut editor,
        &context,
        client.peer(),
        "edit_node",
        json!({"scene_token":token,"node_id":id.to_decimal(),"translation":[1.0,2.0,3.0]}),
    )
    .await;
    assert!(changed.structured_content.unwrap().get("error").is_none());
    let edited = editor.history.scene().clone();
    let undone = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert!(undone.structured_content.unwrap().get("error").is_none());
    assert!(editor.history.can_redo());
    let before = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    let original = editor.history.scene().clone();
    let camera = editor.camera.clone();
    let selection = (editor.selected(), editor.selected_path().to_vec());
    let title = editor.document_title();
    for children in [
        Vec::new(),
        [
            binary_fixture::creation(b"curve", Some(b"object")),
            binary_fixture::chunk(b"NRBC", &[0xff, 0, 7], 8),
        ]
        .concat(),
    ] {
        let mut source = baseline.clone();
        let offset = source.len();
        source.extend(binary_fixture::group(b"NCRV", &children, 4));
        let size = (source.len() - 16) as u64;
        source[8..16].copy_from_slice(&size.to_be_bytes());
        let refusal = azteccraft::native_scene::binary::decode(source.clone().into()).unwrap_err();
        assert_eq!(
            refusal.kind(),
            azteccraft::native_scene::binary::ErrorKind::Unsupported
        );
        assert_eq!(refusal.offset(), Some(offset));
        let rejected = directory.path().join("unsupported.mb");
        std::fs::write(&rejected, &source).unwrap();
        assert!(matches!(
            editor.apply_operation(Operation::OpenScene {
                path: rejected.clone()
            }),
            Err(OperationError::File(_))
        ));
        let failed = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":rejected.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(failed["error"]["code"], "file_error");
        assert_eq!(editor.history.scene(), &original);
        assert_eq!(editor.camera, camera);
        assert_eq!(
            (editor.selected(), editor.selected_path()),
            (selection.0, selection.1.as_slice())
        );
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.document_title(), title);
        assert!(!editor.is_dirty());
        assert!(!editor.files.is_modal());
        assert!(!editor.files.is_closing());
        assert!(std::fs::read(&path).unwrap() == baseline);
        assert!(std::fs::read(&rejected).unwrap() == source);
        let after = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(after, before);
    }
    let redone = call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(redone.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.history.scene(), &edited);
    let undone = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(undone.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.history.scene(), &original);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn ascii_sdk_declared_units_open_inspect_and_curves_in_other_linear_units_refuse() {
    let directory = tempfile::tempdir().unwrap();
    let header = "//Maya ASCII 2016 scene\nrequires maya \"2016\";\n";
    let node = "createNode transform -n \"t1\"; setAttr \".t\" -type \"double3\" 1 2 3;\n";
    let unit_path = directory.path().join("units.ma");
    std::fs::write(
        &unit_path,
        format!("{header}currentUnit -l meter -a radian -t ntsc;\n{node}"),
    )
    .unwrap();
    let curve_path = directory.path().join("meter_curve.ma");
    let curve = String::from_utf8(ascii_fixture::specimen())
        .unwrap()
        .replace("-l centimeter", "-l meter");
    std::fs::write(&curve_path, curve.as_bytes()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let baseline = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":curve_path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "file_error"
    );
    assert_eq!(editor.history.scene(), &baseline);
    assert_eq!(editor.scene_token.to_string(), token);
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":unit_path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
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
    let units = &inspected["inspection"]["units"];
    assert_eq!(units["linear"], "m");
    assert_eq!(units["angular"], "rad");
    assert_eq!(units["time"], "ntsc");
    assert_eq!(inspected["inspection"]["document_origin"], "maya_ascii");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn ascii_sdk_degree_form_curves_open_inspect_and_frame() {
    let directory = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for (name, degree, form, cc, cvs, knots, domain) in [
        (
            "degree1",
            1,
            "open",
            "1 3 0 no 3 4 0 1 2 3 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            4,
            4,
            [0.0, 3.0],
        ),
        (
            "degree2",
            2,
            "open",
            "2 2 0 no 3 5 0 0 1 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            4,
            5,
            [0.0, 2.0],
        ),
        (
            "periodic",
            3,
            "periodic",
            "3 4 2 no 3 9 0 1 2 3 4 5 6 7 8 7 0 0 0 4 0 1 4 4 2 0 4 3 0 0 0 4 0 1 4 4 2",
            7,
            9,
            [2.0, 6.0],
        ),
        (
            "closed",
            3,
            "closed",
            "3 2 1 no 3 7 0 0 0 1 2 2 2 5 0 0 0 2 0 0 2 2 0 0 2 0 0 0 0",
            5,
            7,
            [0.0, 2.0],
        ),
    ] {
        let path = directory.path().join(format!("{name}.ma"));
        std::fs::write(
            &path,
            format!(
                "//Maya ASCII 2016 scene\nrequires maya \"2016\";\n\
                 currentUnit -l centimeter -a degree -t film;\n\
                 createNode transform -n \"c1\";\ncreateNode nurbsCurve -n \"cShape1\" -p \"c1\";\n\
                 setAttr \".cc\" -type \"nurbsCurve\" {cc};\n"
            ),
        )
        .unwrap();
        let token = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await;
        assert_eq!(
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let token = editor.scene_token.to_string();
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
        let curve = &inspected["inspection"]["curves"][0];
        assert_eq!(curve["degree"], degree);
        assert_eq!(curve["form"], form);
        assert_eq!(curve["domain"], json!(domain));
        assert_eq!(curve["control_vertices"], cvs);
        assert_eq!(curve["knots"], knots);
        assert_eq!(curve["draw_available"], true);
        assert_eq!(curve["read_only"], true);
        let id = inspected["inspection"]["nodes"][0]["id"].as_str().unwrap();
        let path_value = inspected["inspection"]["nodes"][0]["instance_paths"][0].clone();
        call(
            &mut editor,
            &context,
            client.peer(),
            "select_node",
            json!({"scene_token":token,"node_id":id,"instance_path":path_value}),
        )
        .await;
        let framed = call(
            &mut editor,
            &context,
            client.peer(),
            "frame_selected",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(framed.is_error, Some(false));
        let before = editor.history.scene().clone();
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "rename_node",
            json!({"scene_token":token,"node_id":id,"name":"nope"}),
        )
        .await;
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &before);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn ascii_sdk_inspect_reports_inert_reference_declarations_without_private_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.ma");
    let mut scene = ascii_fixture::specimen();
    scene.extend_from_slice(
        b"\nfile -r -ns \"cube\" -dr 1 -rfn \"cubeRN\" \"assets/cube.ma\";\nfile -r \"/Users/someone/private/shot.ma\";\nfile -r \"../outside.ma\";\n",
    );
    std::fs::write(&path, scene).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let baseline = editor.history.scene().clone();
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
    let references = &inspected["inspection"]["references"];
    assert_eq!(
        *references,
        json!([
            {"path_kind":"relative","path":"assets/cube.ma","namespace":"cube",
             "reference_node":"cubeRN","renaming_prefix":null,"file_type":null,
             "deferred":true,"depth_info":null},
            {"path_kind":"absolute","path":null,"namespace":null,"reference_node":null,
             "renaming_prefix":null,"file_type":null,"deferred":null,"depth_info":null},
            {"path_kind":"escaping","path":null,"namespace":null,"reference_node":null,
             "renaming_prefix":null,"file_type":null,"deferred":null,"depth_info":null}
        ])
    );
    let text = serde_json::to_string(&inspected).unwrap();
    assert!(!text.contains("someone") && !text.contains("outside.ma"));
    assert_eq!(editor.history.scene(), &baseline);
    assert!(!editor.is_dirty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
