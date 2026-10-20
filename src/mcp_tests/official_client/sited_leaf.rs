//! Over MCP, a sited node in an opened Maya Binary scene gets its absent translate leaf inserted
//! on Save (issue #19); rotate, scale and every other node still refuse with `absent_channel_leaf`.
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

fn sited_bytes() -> Vec<u8> {
    sited::sited_scene(&binary_fixture::mesh_payload(&[0, 1, 0x6000_0002]))
}

struct Rig {
    editor: Editor,
    context: egui::Context,
    client: rmcp::service::RunningService<RoleClient, ()>,
    server: tokio::task::JoinHandle<()>,
    _directory: tempfile::TempDir,
    source: std::path::PathBuf,
    original: Vec<u8>,
    token: String,
}

/// The two transforms of the fixture, as MCP arguments.
struct Node {
    id: azteccraft::scene::NodeId,
    node: String,
    path: Vec<String>,
}

impl Rig {
    async fn open() -> Self {
        let original = sited_bytes();
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("sited.mb");
        std::fs::write(&source, &original).unwrap();
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
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let token = editor.scene_token.to_string();
        Self {
            editor,
            context,
            client,
            server,
            _directory: directory,
            source,
            original,
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

    fn node(&self, name: &str) -> Node {
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
        Node {
            id,
            node: id.to_decimal(),
            path: path.iter().map(|id| id.to_decimal()).collect(),
        }
    }

    async fn select(&mut self, node: &Node) {
        let args = json!({"scene_token":self.token,"node_id":node.node,"instance_path":node.path});
        self.call("select_node", args).await;
    }

    fn begin(&self, node: &Node, rest: Value) -> Value {
        let mut args =
            json!({"scene_token":self.token,"node_id":node.node,"instance_path":node.path});
        args.as_object_mut()
            .unwrap()
            .extend(rest.as_object().unwrap().clone());
        args
    }

    fn translation_bits(&self, node: &Node) -> [u64; 3] {
        self.editor
            .history
            .scene()
            .internal_transform(node.id)
            .unwrap()
            .translation_cm
            .map(f64::to_bits)
    }

    /// Everything a failed or refused operation must leave alone.
    fn baseline(&self) -> (NativeScene, bool, bool, bool, String) {
        (
            self.editor.history.scene().clone(),
            self.editor.is_dirty(),
            self.editor.history.can_undo(),
            self.editor.history.can_redo(),
            self.editor.scene_token.to_string(),
        )
    }

    async fn finish(self) {
        self.client.cancel().await.unwrap();
        drop(self.editor);
        self.server.await.unwrap();
    }
}

use azteccraft::native_scene::NativeScene;

fn inserted_record(values: [f64; 3]) -> Vec<u8> {
    sited::triple_aux(b"t", sited::SITE_AUX, values)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[tokio::test]
async fn move_over_mcp_inserts_the_translate_leaf_on_save_and_reopens_bit_equal() {
    let mut rig = Rig::open().await;
    let object = rig.node("object");
    rig.select(&object).await;
    assert_eq!(rig.translation_bits(&object), [0; 3]);
    let begin = rig
        .call(
            "begin_manipulation",
            rig.begin(&object, json!({"kind":"move","axis":"x"})),
        )
        .await
        .structured_content
        .unwrap();
    assert_eq!(begin["outcome"], "manipulation_started");
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let update = rig
        .call(
            "update_manipulation",
            json!({"scene_token":rig.token,"manipulation_id":id,"distance":3.5}),
        )
        .await;
    assert!(!update.is_error.unwrap_or(false));
    assert_eq!(
        rig.translation_bits(&object),
        [3.5f64.to_bits(), 0, 0],
        "the preview carries the value"
    );
    let committed = rig
        .call(
            "finish_manipulation",
            json!({"scene_token":rig.token,"manipulation_id":id,"finish":"commit"}),
        )
        .await;
    assert!(!committed.is_error.unwrap_or(false));
    assert!(rig.editor.is_dirty());
    assert_eq!(rig.editor.history.undo_label(), Some("Move"));
    #[cfg(unix)]
    {
        let first = rig.source.parent().unwrap().join("first.mb");
        let saved = rig
            .call(
                "save_scene_as",
                json!({"scene_token":rig.token,"path":first}),
            )
            .await;
        assert_eq!(saved.structured_content.unwrap()["outcome"], "saved");
        let written = std::fs::read(&first).unwrap();
        assert_eq!(written.len(), rig.original.len() + 48);
        assert!(contains(&written, &inserted_record([3.5, 0.0, 0.0])));
        // The save leaves the original file alone.
        assert_eq!(std::fs::read(&rig.source).unwrap(), rig.original);
        // A repeat Save of the same state rewrites the same bytes.
        let repeated = rig
            .call("save_scene", json!({"scene_token":rig.token}))
            .await;
        assert_eq!(repeated.structured_content.unwrap()["outcome"], "saved");
        assert_eq!(std::fs::read(&first).unwrap(), written);
        let reopened = rig
            .call("open_scene", json!({"scene_token":rig.token,"path":first}))
            .await;
        assert_eq!(
            reopened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        rig.token = rig.editor.scene_token.to_string();
        let again = rig.node("object");
        assert_eq!(
            rig.translation_bits(&again),
            [3.5f64.to_bits(), 0, 0],
            "reopened scene decodes the inserted leaf"
        );
        assert!(!rig.editor.is_dirty());
        // The reopened leaf is now present, so it is an ordinary edit that keeps the size.
        let edited = rig
            .call(
                "edit_node",
                json!({"scene_token":rig.token,"node_id":again.node,"translation":[-0.0,null,null]}),
            )
            .await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    }
    rig.finish().await;
}

#[tokio::test]
async fn edit_node_translation_over_mcp_inserts_the_leaf_with_undo_and_redo() {
    let mut rig = Rig::open().await;
    let object = rig.node("object");
    let before = rig.baseline();
    let edited = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":object.node,"translation":[1.25,null,-2.5]}),
        )
        .await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    let after = rig.baseline();
    assert_eq!(
        rig.translation_bits(&object),
        [1.25f64.to_bits(), 0, (-2.5f64).to_bits()]
    );
    rig.call("undo", json!({"scene_token":rig.token})).await;
    assert_eq!(rig.editor.history.scene(), &before.0);
    assert!(!rig.editor.is_dirty());
    rig.call("redo", json!({"scene_token":rig.token})).await;
    assert_eq!(rig.editor.history.scene(), &after.0);
    #[cfg(unix)]
    {
        let destination = rig.source.parent().unwrap().join("edited.mb");
        let saved = rig
            .call(
                "save_scene_as",
                json!({"scene_token":rig.token,"path":destination}),
            )
            .await;
        assert_eq!(saved.structured_content.unwrap()["outcome"], "saved");
        let written = std::fs::read(&destination).unwrap();
        assert_eq!(written.len(), rig.original.len() + 48);
        assert!(contains(&written, &inserted_record([1.25, 0.0, -2.5])));
        rig.call(
            "open_scene",
            json!({"scene_token":rig.token,"path":destination}),
        )
        .await;
        rig.token = rig.editor.scene_token.to_string();
        let reopened = rig.node("object");
        assert_eq!(
            rig.translation_bits(&reopened),
            [1.25f64.to_bits(), 0, (-2.5f64).to_bits()]
        );
    }
    // An all-zero translation is elided: nothing to insert, so nothing to save.
    let other = rig.node("other");
    let unchanged = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":other.node,"translation":[0,null,null]}),
        )
        .await;
    assert_eq!(
        unchanged.structured_content.unwrap()["outcome"],
        "unchanged"
    );
    rig.finish().await;
}

#[tokio::test]
async fn rotate_scale_and_siteless_edits_over_mcp_still_refuse_with_absent_channel_leaf() {
    let mut rig = Rig::open().await;
    let object = rig.node("object");
    let other = rig.node("other");
    rig.select(&object).await;
    let before = rig.baseline();
    // `object` has a rotate leaf but no scale leaf; `other` is the reverse.
    for (node, rest) in [
        (
            &object,
            json!({"kind":"scale","handle":"x","space":"object"}),
        ),
        (&other, json!({"kind":"rotate","axis":"x","space":"object"})),
    ] {
        rig.select(node).await;
        let refused = rig
            .call("begin_manipulation", rig.begin(node, rest.clone()))
            .await;
        let error = refused.structured_content.unwrap()["error"].clone();
        assert_eq!(error["code"], "absent_channel_leaf", "{rest}");
        assert_eq!(rig.baseline(), before);
        assert!(rig.editor.history.active_manipulation().is_none());
    }
    // Inspection agrees: on `object` Move (insertable) and Rotate are available, Scale is not.
    rig.select(&object).await;
    let kinds = rig
        .call("inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["manipulation"]["kinds"]
        .clone();
    let available = |name: &str| {
        kinds
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["kind"] == name)
            .unwrap()["available"]
            .clone()
    };
    assert_eq!(
        [available("move"), available("rotate"), available("scale")],
        [true, true, false]
    );
    for (node, field, value) in [
        (&object, "scale", json!([2, null, null])),
        (&other, "rotation_degrees", json!([30, null, null])),
    ] {
        let mut args = json!({"scene_token":rig.token,"node_id":node.node});
        args[field] = value;
        let refused = rig.call("edit_node", args).await;
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(rig.baseline(), before);
    }
    assert_eq!(std::fs::read(&rig.source).unwrap(), rig.original);
    rig.finish().await;
}
