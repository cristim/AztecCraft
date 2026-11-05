use super::binary_fixture::{creation, group, scene, triple};
use super::selection::grouped_scene_bytes_with;
use super::*;

fn root(name: &[u8]) -> Vec<u8> {
    group(b"XFRM", &creation(name, None), 4)
}

fn group_node() -> Vec<u8> {
    group(
        b"XFRM",
        &[creation(b"group", None), triple(b"t", [2., 0., 0.])].concat(),
        4,
    )
}

/// `group` > `child` > `shape` (see `grouped_scene_bytes`) followed by a plain root `other`.
async fn opened(
    context: &egui::Context,
) -> (
    Editor,
    rmcp::service::RunningService<RoleClient, ()>,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("grouped.mb");
    std::fs::write(&source, grouped_scene_bytes_with(&root(b"other"))).unwrap();
    let (mut editor, client, server) = start(context).await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":source}),
    )
    .await;
    (editor, client, server, directory)
}

async fn save_as(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    path: &std::path::Path,
) -> Vec<u8> {
    let token = editor.scene_token.to_string();
    let saved = call(
        editor,
        context,
        peer,
        "save_scene_as",
        json!({"scene_token":token,"path":path}),
    )
    .await;
    assert_ne!(saved.is_error, Some(true), "{saved:?}");
    std::fs::read(path).unwrap()
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn sdk_delete_node_of_a_group_and_of_a_child_then_save_as_and_undo() {
    let context = egui::Context::default();
    for (index, left, expected) in [
        (0, 1, scene(&root(b"other"))),
        (1, 2, scene(&[group_node(), root(b"other")].concat())),
    ] {
        let (mut editor, client, server, directory) = opened(&context).await;
        let paths = editor.history.scene().transform_paths().unwrap();
        let before = editor.history.scene().clone();
        editor.set_selection_members_for_test(vec![paths[2].clone()]);
        editor.sync_fields();
        frame(&mut editor, &context, vec![]);
        let token = editor.scene_token.to_string();
        let reply = call(
            &mut editor,
            &context,
            client.peer(),
            "delete_node",
            json!({"scene_token":token,"node_id":paths[index].node.to_decimal()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(reply["outcome"], "changed", "{reply}");
        assert_eq!(editor.history.scene().nodes().len(), left);
        // A surviving selected sibling stays selected.
        assert_eq!(editor.selection.members(), vec![paths[2].clone()]);
        let out = directory.path().join("out.mb");
        assert_eq!(
            save_as(&mut editor, &context, client.peer(), &out).await,
            expected.to_vec()
        );
        let token = editor.scene_token.to_string();
        let undo = call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_ne!(undo.is_error, Some(true));
        assert_eq!(editor.history.scene().nodes().len(), before.nodes().len());
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn sdk_delete_selected_with_an_ancestor_and_its_descendant_deletes_the_subtree_once() {
    let context = egui::Context::default();
    let (mut editor, client, server, directory) = opened(&context).await;
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(vec![paths[1].clone(), paths[0].clone()]);
    editor.sync_fields();
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let reply = call(
        &mut editor,
        &context,
        client.peer(),
        "delete_selected",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(reply["outcome"], "changed", "{reply}");
    assert_eq!(editor.history.scene().nodes().len(), 1);
    assert!(editor.selection.is_empty());
    let out = directory.path().join("out.mb");
    assert_eq!(
        save_as(&mut editor, &context, client.peer(), &out).await,
        scene(&root(b"other")).to_vec()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
