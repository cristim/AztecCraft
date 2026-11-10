//! Issue #39 slice B over the official client: the refusals that depend on the document, keys and
//! the opened Maya file formats.
use super::pivot_edit::{Client, code, cubes, inspected, reply, tool};
use super::*;

#[tokio::test]
async fn a_keyed_node_refuses_both_tools_and_an_unkeyed_sibling_still_works() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 2);
    let keyed = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids":[node]}),
    )
    .await;
    assert_eq!(reply(&keyed)["outcome"], "changed");
    let scene = editor.history.scene().clone();
    let generation =
        inspected(&mut editor, &context, &client).await["selection_generation"].clone();
    let set = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3]}),
    )
    .await;
    assert_eq!(code(&set), "pivot_on_keyed_node");
    let reset = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&reset), "pivot_on_keyed_node");
    assert_eq!(editor.history.scene(), &scene);
    let sibling = editor.history.scene().nodes()[1].id().to_decimal();
    let ok = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":sibling,"rotate_pivot":[1,2,3]}),
    )
    .await;
    assert_eq!(reply(&ok)["outcome"], "changed");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_maya_ascii_scene_is_read_only_for_both_tools() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("scene.ma");
    std::fs::write(&source, ascii_fixture::specimen()).unwrap();
    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":source}),
    )
    .await;
    assert_eq!(reply(&opened)["outcome"], "document_applied");
    let node = editor.history.scene().nodes()[0].id().to_decimal();
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let scene = editor.history.scene().clone();
    for (name, arguments) in [
        ("set_pivots", json!({"node_id":node,"rotate_pivot":[1,2,3]})),
        ("reset_pivots", json!({"mode":"zero"})),
    ] {
        let refused = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(code(&refused), "read_only_format", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

fn rotated_pivoted_mb() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("pivoted.mb");
    std::fs::write(&source, super::pivots::pivoted_bytes()).unwrap();
    (directory, source)
}

async fn open(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Client,
    path: &std::path::Path,
) -> String {
    let opened = tool(editor, context, client, "open_scene", json!({"path":path})).await;
    assert_eq!(reply(&opened)["outcome"], "document_applied");
    let node = editor.history.scene().nodes()[0].id().to_decimal();
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    node
}

#[tokio::test]
async fn an_opened_maya_binary_refuses_a_compensating_edit_and_accepts_the_exact_ones() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (_directory, source) = rotated_pivoted_mb();
    let node = open(&mut editor, &context, &client, &source).await;
    let scene = editor.history.scene().clone();
    let compensating = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3]}),
    )
    .await;
    assert_eq!(code(&compensating), "pivot_needs_translate_correction");
    assert_eq!(editor.history.scene(), &scene);

    let raw = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3],"scale_pivot":[4,5,6],"preserve":false}),
    )
    .await;
    assert_eq!(reply(&raw)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    assert_eq!(
        seen["nodes"][0]["transform_attributes"]["rotate_pivot"],
        json!([1.0, 2.0, 3.0])
    );
    assert_eq!(
        seen["nodes"][0]["transform_attributes"]["scale_pivot"],
        json!([4.0, 5.0, 6.0])
    );

    let zero = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero"}),
    )
    .await;
    assert_eq!(reply(&zero)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    assert_eq!(seen["nodes"][0]["transform_attributes"], Value::Null);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_maya_binary_node_without_the_pivot_leaf_refuses_a_nonzero_pivot() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("specimen.mb");
    std::fs::write(&source, binary_fixture::specimen()).unwrap();
    let node = open(&mut editor, &context, &client, &source).await;
    let scene = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3],"preserve":false}),
    )
    .await;
    assert_eq!(code(&refused), "absent_pivot_leaf");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn a_saved_pivot_edit_reaches_the_file_and_a_changed_source_is_not_overwritten() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (_directory, source) = rotated_pivoted_mb();
    let node = open(&mut editor, &context, &client, &source).await;
    let edit = |rp: [f64; 3]| json!({"node_id":node,"rotate_pivot":rp,"preserve":false});
    tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        edit([1.0, 2.0, 3.0]),
    )
    .await;
    let saved = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(reply(&saved)["outcome"], "saved");
    let written = azteccraft::document::load_native(&source).unwrap();
    assert_eq!(written.scene.nodes()[0].rotate_pivot(), [1.0, 2.0, 3.0]);

    let mut foreign = std::fs::read(&source).unwrap();
    foreign.push(0);
    std::fs::write(&source, &foreign).unwrap();
    tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        edit([7.0, 8.0, 9.0]),
    )
    .await;
    let edited = editor.history.scene().clone();
    let changed = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(code(&changed), "source_changed");
    assert_eq!(std::fs::read(&source).unwrap(), foreign);
    assert!(editor.is_dirty());
    assert_eq!(
        editor.history.scene(),
        &edited,
        "the refused Save restores nothing"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
