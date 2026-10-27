//! Over MCP, a sited node in an opened Maya Binary scene gets its absent translate leaf inserted
//! on Save (issue #19); rotate, scale and every other node still refuse with `absent_channel_leaf`.
use super::*;
use crate::editor_operations::FileDecision;
use azteccraft::native_scene::NativeScene;

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

/// The sited scene plus `locked`: a transform whose first leaf is a rotate pivot (`rp`), so the file
/// records no site for it and its absent translate leaf cannot be inserted.
fn siteless_bytes() -> Vec<u8> {
    let sited = sited_bytes();
    let tree = azteccraft::maya_binary::container::inspect(sited.clone().into()).unwrap();
    let locked = sited::sited_transform(
        b"locked",
        &[sited::triple_aux(b"rp", sited::SITE_AUX, [0.0; 3])],
    );
    // Insert before the DMSH group, then fix the root size word (the only ancestor).
    let at = tree
        .chunks()
        .iter()
        .find(|chunk| chunk.tag() == *b"FOR8" && &sited[chunk.payload_range()][..4] == b"DMSH")
        .unwrap()
        .header_range()
        .start;
    let mut bytes = sited[..at].to_vec();
    bytes.extend(&locked);
    bytes.extend(&sited[at..]);
    let size = (bytes.len() as u64 - 16).to_be_bytes();
    bytes[8..16].copy_from_slice(&size);
    bytes
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
        Self::open_bytes(sited_bytes()).await
    }

    async fn open_bytes(original: Vec<u8>) -> Self {
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

#[cfg(unix)]
fn inserted_record(values: [f64; 3]) -> Vec<u8> {
    sited::triple_aux(b"t", sited::SITE_AUX, values)
}

#[cfg(unix)]
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
    assert_eq!(rig.editor.history.undo_label(), Some("Move".to_string()));
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
        let error = refused.structured_content.unwrap()["error"].clone();
        assert_eq!(error["code"], "scene_error", "{field}");
        assert_eq!(
            error["message"], "native ownership or edit effects are unsupported",
            "{field}"
        );
        assert_eq!(rig.baseline(), before);
    }
    assert_eq!(std::fs::read(&rig.source).unwrap(), rig.original);
    rig.finish().await;
}

#[tokio::test]
async fn a_node_without_a_recorded_site_still_refuses_translate_over_mcp() {
    let mut rig = Rig::open_bytes(siteless_bytes()).await;
    let locked = rig.node("locked");
    rig.select(&locked).await;
    let before = rig.baseline();
    let refused = rig
        .call(
            "begin_manipulation",
            rig.begin(&locked, json!({"kind":"move","axis":"x"})),
        )
        .await;
    let error = refused.structured_content.unwrap()["error"].clone();
    assert_eq!(error["code"], "absent_channel_leaf");
    assert_eq!(
        error["message"],
        "translateX is absent from the Maya binary source and cannot be edited; leaf insertion is unsupported"
    );
    assert!(rig.editor.history.active_manipulation().is_none());
    assert_eq!(rig.baseline(), before);
    let refused = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":locked.node,"translation":[2,null,null]}),
        )
        .await;
    let error = refused.structured_content.unwrap()["error"].clone();
    assert_eq!(error["code"], "scene_error");
    assert_eq!(
        error["message"],
        "native ownership or edit effects are unsupported"
    );
    assert_eq!(rig.baseline(), before);
    assert_eq!(std::fs::read(&rig.source).unwrap(), rig.original);
    rig.finish().await;
}

impl Rig {
    async fn code(&mut self, name: &'static str, arguments: Value) -> String {
        let result = self.call(name, arguments).await;
        result
            .structured_content
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: {result:?}"))["error"]["code"]
            .as_str()
            .map_or_else(|| format!("no error: {result:?}"), str::to_owned)
    }
}

type Setup = fn(&mut Editor, &egui::Context);

fn pending_confirmation(editor: &mut Editor, operation: Operation) {
    assert!(matches!(
        editor.apply_operation(operation).unwrap(),
        OperationReply::PendingConfirmation { .. }
    ));
}

#[tokio::test]
async fn sited_move_and_edit_node_share_the_guard_matrix_and_preserve_the_baseline() {
    let mut rig = Rig::open().await;
    let object = rig.node("object");
    rig.select(&object).await;
    // A dirty document, so New and Close raise confirmations.
    let dirtied = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":object.node,"translation":[1,null,null]}),
        )
        .await;
    assert_eq!(dirtied.structured_content.unwrap()["outcome"], "changed");
    assert!(rig.editor.is_dirty());
    let cases: [(&str, &str, Setup); 6] = [
        ("busy_editing", "text focus", |_, context| {
            context.memory_mut(|memory| memory.request_focus(name_edit_id()));
        }),
        ("busy_editing", "invalid draft", |editor, _| {
            editor.channels[0] = "not a number".into();
        }),
        ("busy_editing", "valid draft", |editor, _| {
            editor.channels[0] = "5".into();
        }),
        ("busy_editing", "name draft", |editor, _| {
            editor.name = "draft name".into();
        }),
        ("busy_editing", "modal", |editor, _| {
            pending_confirmation(editor, Operation::NewScene);
        }),
        ("busy_editing", "close in progress", |editor, _| {
            pending_confirmation(editor, Operation::RequestClose);
            let pending = editor.files.pending_inspection().unwrap();
            editor
                .apply_operation(Operation::ResolveFileAction {
                    pending_id: pending.id.parse().unwrap(),
                    decision: FileDecision::Discard,
                })
                .unwrap();
            assert!(editor.files.is_closing());
        }),
    ];
    for (expected, label, setup) in cases {
        // Each case starts from a fresh editor state: reopen the document and redo the edit.
        let mut case = Rig::open().await;
        let object = case.node("object");
        case.select(&object).await;
        case.call(
            "edit_node",
            json!({"scene_token":case.token,"node_id":object.node,"translation":[1,null,null]}),
        )
        .await;
        setup(&mut case.editor, &case.context);
        frame(&mut case.editor, &case.context, vec![]);
        let before = case.baseline();
        let moved = case.begin(&object, json!({"kind":"move","axis":"x"}));
        let editing =
            json!({"scene_token":case.token,"node_id":object.node,"translation":[7,null,null]});
        assert_eq!(
            case.code("begin_manipulation", moved).await,
            expected,
            "{label}"
        );
        assert_eq!(
            case.code("edit_node", editing.clone()).await,
            expected,
            "{label}"
        );
        assert_eq!(case.baseline(), before, "{label}");
        assert!(
            case.editor.history.active_manipulation().is_none(),
            "{label}"
        );
        // A stale token is rejected the same way, ahead of every other guard.
        let stale = json!({"scene_token":Uuid::new_v4().to_string(),"node_id":object.node,"translation":[7,null,null]});
        assert_eq!(
            case.code("edit_node", stale).await,
            "stale_scene",
            "{label}"
        );
        assert_eq!(case.baseline(), before, "{label}");
        assert_eq!(std::fs::read(&case.source).unwrap(), case.original);
        case.finish().await;
    }
    // Pending native input is cleared by every frame before a bridged call runs, so it is checked on
    // the shared remote pipeline directly, with the same two operations.
    let mut case = Rig::open().await;
    let object = case.node("object");
    case.select(&object).await;
    case.editor.native_input_pending = true;
    let before = case.baseline();
    let transform = case
        .editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .remove(0);
    for operation in [
        Operation::BeginMove {
            target: azteccraft::native_scene::TransformTarget {
                node: object.id,
                path: transform.path.clone(),
            },
            axis: azteccraft::native_scene::WorldAxis::X,
        },
        Operation::EditNode {
            id: object.id,
            name: None,
            translation: [Some(7.0), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        },
    ] {
        let result = case.editor.remote_operation_guarded(
            crate::editor_operations::RemoteRequest {
                expected_scene_token: Some(case.editor.scene_token),
                operation,
            },
            &case.context,
            &|| true,
        );
        assert!(matches!(result, Err(OperationError::BusyEditing)));
        assert_eq!(case.baseline(), before);
    }
    case.finish().await;
    // With nothing pending the dirty document accepts an edit, and a refused edit then fails atomically.
    let again = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":object.node,"translation":[null,4,null]}),
        )
        .await;
    assert_eq!(again.structured_content.unwrap()["outcome"], "changed");
    let before = rig.baseline();
    let refused = rig
        .call(
            "edit_node",
            json!({"scene_token":rig.token,"node_id":object.node,"translation":[9,null,null],"scale":[3,null,null]}),
        )
        .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(
        rig.baseline(),
        before,
        "a failed edit preserves its baseline"
    );
    rig.finish().await;
}

#[tokio::test]
async fn delete_node_on_an_opened_binary_root_leaf_is_refused_without_mutation() {
    let mut rig = Rig::open_bytes(siteless_bytes()).await;
    let baseline = rig.baseline();
    for name in ["object", "locked"] {
        let node = rig.node(name);
        let refused = rig
            .call(
                "delete_node",
                json!({"scene_token":rig.token,"node_id":node.node}),
            )
            .await;
        let error = &refused.structured_content.unwrap()["error"];
        assert_eq!(error["code"], "scene_error", "{name}");
        // The code alone also matches ReadOnlyFormat; the message pins UnsupportedEdit.
        assert_eq!(
            error["message"], "native ownership or edit effects are unsupported",
            "{name}"
        );
        assert_eq!(rig.baseline(), baseline, "{name}");
    }
    rig.finish().await;
}
