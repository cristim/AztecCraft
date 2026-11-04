//! Over MCP, Delete on an opened Maya Binary: typed refusals leave the scene alone, and a
//! successful Delete saves a file that reopens without the node (issue #272, PROVISIONAL).
use super::*;

#[allow(dead_code)]
mod sited {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sites.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sited.rs"
    ));
}

struct Rig {
    editor: Editor,
    context: egui::Context,
    client: rmcp::service::RunningService<RoleClient, ()>,
    server: tokio::task::JoinHandle<()>,
    directory: tempfile::TempDir,
    token: String,
}

impl Rig {
    async fn open(bytes: Vec<u8>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("scene.mb");
        std::fs::write(&source, &bytes).unwrap();
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let old = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":old,"path":source}),
        )
        .await;
        assert_eq!(
            opened.structured_content.as_ref().unwrap()["outcome"],
            "document_applied",
            "{opened:?}"
        );
        let token = editor.scene_token.to_string();
        Self {
            editor,
            context,
            client,
            server,
            directory,
            token,
        }
    }

    async fn call(&mut self, name: &'static str, arguments: Value) -> CallToolResult {
        call(
            &mut self.editor,
            &self.context,
            self.client.peer(),
            name,
            arguments,
        )
        .await
    }

    fn node(&self, name: &str) -> (String, Vec<String>) {
        let scene = self.editor.history.scene();
        let id = scene
            .nodes()
            .iter()
            .find(|n| n.name() == name)
            .unwrap()
            .id();
        let path = scene
            .transform_paths()
            .unwrap()
            .into_iter()
            .find(|target| target.node == id)
            .unwrap()
            .path;
        (
            id.to_decimal(),
            path.iter().map(|id| id.to_decimal()).collect(),
        )
    }

    fn has(&self, name: &str) -> bool {
        let scene = self.editor.history.scene();
        scene.nodes().iter().any(|n| n.name() == name)
    }

    async fn select(&mut self, name: &str) {
        let (node, path) = self.node(name);
        let args = json!({"scene_token":self.token,"node_id":node,"instance_path":path});
        self.call("select_node", args).await;
    }

    async fn finish(self) {
        self.client.cancel().await.unwrap();
        drop(self.editor);
        self.server.await.unwrap();
    }
}

fn sited_bytes() -> Vec<u8> {
    sited::sited_scene(&binary_fixture::mesh_payload(&[0, 1, 0x6000_0002]))
}

/// A root `a` with a child transform `c`, so deleting `a` is refused.
fn parent_with_child() -> Vec<u8> {
    use binary_fixture::{creation, group, scene, triple};
    let node = |name: &[u8], parent: Option<&[u8]>| {
        group(
            b"XFRM",
            &[creation(name, parent), triple(b"t", [1.0, 2.0, 3.0])].concat(),
            4,
        )
    };
    scene(&[node(b"a", None), node(b"c", Some(b"a"))].concat()).to_vec()
}

fn error(result: &CallToolResult) -> &Value {
    &result.structured_content.as_ref().unwrap()["error"]
}

#[tokio::test]
async fn delete_of_a_node_with_a_child_is_a_typed_refusal_that_changes_nothing() {
    let mut rig = Rig::open(parent_with_child()).await;
    let (node, _) = rig.node("a");
    let before = rig.editor.history.scene().clone();
    let refused = rig
        .call(
            "delete_node",
            json!({"scene_token":rig.token,"node_id":node}),
        )
        .await;
    assert_eq!(error(&refused)["code"], "unsupported_delete");
    assert!(
        error(&refused)["message"]
            .as_str()
            .unwrap()
            .contains("children")
    );
    rig.select("a").await;
    let selected = rig
        .call("delete_selected", json!({"scene_token":rig.token}))
        .await;
    assert_eq!(error(&selected)["code"], "unsupported_delete");
    assert_eq!(rig.editor.history.scene(), &before);
    assert!(!rig.editor.is_dirty());
    assert!(!rig.editor.history.can_undo());
    rig.finish().await;
}

#[tokio::test]
async fn delete_selected_on_an_opened_binary_saves_a_file_that_reopens_without_the_node() {
    let mut rig = Rig::open(sited_bytes()).await;
    rig.select("other").await;
    let deleted = rig
        .call("delete_selected", json!({"scene_token":rig.token}))
        .await;
    assert_eq!(
        deleted.structured_content.as_ref().unwrap()["outcome"],
        "changed",
        "{deleted:?}"
    );
    assert!(!rig.has("other"));
    assert!(rig.editor.is_dirty());
    let saved_path = rig.directory.path().join("saved.mb");
    let saved = rig
        .call(
            "save_scene_as",
            json!({"scene_token":rig.token,"path":saved_path}),
        )
        .await;
    assert_eq!(saved.structured_content.unwrap()["outcome"], "saved");
    let reopened = rig
        .call(
            "open_scene",
            json!({"scene_token":rig.token,"path":saved_path}),
        )
        .await;
    assert_eq!(
        reopened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    rig.token = rig.editor.scene_token.to_string();
    assert!(!rig.has("other"));
    assert!(rig.has("object"));
    rig.finish().await;
}

#[tokio::test]
async fn after_a_delete_an_absent_leaf_is_refused_and_undo_restores_the_edit() {
    let mut rig = Rig::open(sited_bytes()).await;
    let (object, path) = rig.node("object");
    rig.select("other").await;
    rig.call("delete_selected", json!({"scene_token":rig.token}))
        .await;
    assert!(!rig.has("other"));
    let before = rig.editor.history.scene().clone();
    let edit = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":object,"translation":[1.0,null,null]}),
        )
        .await;
    assert_eq!(error(&edit)["code"], "scene_error");
    assert!(
        error(&edit)["message"]
            .as_str()
            .unwrap()
            .contains("after a delete")
    );
    rig.select("object").await;
    let begin = rig
        .call(
            "begin_manipulation",
            json!({"scene_token":rig.token,"node_id":object,"instance_path":path,"kind":"move","axis":"x"}),
        )
        .await;
    assert_eq!(error(&begin)["code"], "absent_channel_leaf", "{begin:?}");
    assert_eq!(rig.editor.history.scene(), &before);
    rig.call("undo", json!({"scene_token":rig.token})).await;
    assert!(rig.has("other"));
    let edit = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":object,"translation":[1.0,null,null]}),
        )
        .await;
    assert_eq!(edit.structured_content.unwrap()["outcome"], "changed");
    rig.finish().await;
}
