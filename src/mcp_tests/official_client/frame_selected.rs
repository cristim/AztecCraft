use super::*;
use azteccraft::viewport::FrameChildren;

const CURVE_CC: &str =
    "setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;";

/// Parent "g" with its own curve and a child "c" 40 units along x, "g" selected.
fn parent_with_far_child(editor: &mut Editor) {
    let source = format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film;\n\
         createNode transform -n \"g\"; createNode nurbsCurve -n \"gShape\" -p \"g\"; {CURVE_CC}\n\
         createNode transform -n \"c\" -p \"g\"; setAttr \".t\" -type \"double3\" 40 0 0;\n\
         createNode nurbsCurve -n \"cShape\" -p \"c\"; {CURVE_CC}"
    );
    let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
    editor.history = azteccraft::history::History::new(scene);
    let node = editor.history.scene().nodes()[0].id();
    let target = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.node == node)
        .unwrap();
    editor.set_selection_members_for_test(vec![target]);
    editor.sync_fields();
}

fn expected(editor: &Editor, children: FrameChildren) -> azteccraft::viewport::Camera {
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let prefix = editor.selection.lead().unwrap().path.clone();
    let mut camera = editor.camera.clone();
    camera
        .frame_view_many(&view, &[prefix.as_slice()], children, editor.aspect)
        .unwrap();
    camera
}

#[tokio::test]
async fn sdk_frame_selected_defaults_to_the_parent_only_and_with_children_matches_ctrl_f() {
    for (arguments, children) in [
        (json!({}), FrameChildren::Exclude),
        (json!({"with_children": false}), FrameChildren::Exclude),
        (json!({"with_children": true}), FrameChildren::Include),
    ] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        parent_with_far_child(&mut editor);
        frame(&mut editor, &context, vec![]);
        let want = expected(&editor, children);
        let mut args = arguments.clone();
        args["scene_token"] = json!(editor.scene_token.to_string());
        let reply = call(&mut editor, &context, client.peer(), "frame_selected", args).await;
        assert_eq!(reply.structured_content.unwrap()["outcome"], "changed");
        assert_eq!(editor.camera, want, "{arguments}");
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    parent_with_far_child(&mut editor);
    frame(&mut editor, &context, vec![]);
    assert_ne!(
        expected(&editor, FrameChildren::Exclude),
        expected(&editor, FrameChildren::Include)
    );
    let mut native = editor.camera.clone();
    editor.native_operation(Operation::FrameSelected {
        with_children: true,
    });
    native = std::mem::replace(&mut editor.camera, native);
    let args = json!({"scene_token": editor.scene_token.to_string(), "with_children": true});
    call(&mut editor, &context, client.peer(), "frame_selected", args).await;
    assert_eq!(editor.camera, native);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_frame_selected_rejects_a_non_boolean_flag_and_keeps_the_camera() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    parent_with_far_child(&mut editor);
    frame(&mut editor, &context, vec![]);
    let before = editor.camera.clone();
    for bad in [json!("yes"), json!(1), json!(null), json!([])] {
        let args = json!({"scene_token": editor.scene_token.to_string(), "with_children": bad});
        let rejected =
            call_response(&mut editor, &context, client.peer(), "frame_selected", args).await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {bad}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS, "{bad}");
        assert_eq!(editor.camera, before, "{bad}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_frame_selected_with_nothing_selected_frames_all_in_object_mode() {
    for arguments in [json!({}), json!({"with_children": true})] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        parent_with_far_child(&mut editor);
        editor.native_operation(Operation::DeselectAll);
        frame(&mut editor, &context, vec![]);
        let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
        let mut want = editor.camera.clone();
        want.frame_view(&view, None, editor.aspect).unwrap();
        let mut args = arguments.clone();
        args["scene_token"] = json!(editor.scene_token.to_string());
        let reply = call(&mut editor, &context, client.peer(), "frame_selected", args).await;
        assert!(reply.structured_content.unwrap().get("error").is_none());
        assert_eq!(editor.camera, want, "{arguments}");
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
