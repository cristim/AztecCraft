//! Issue #39 slice A over the official client: Move, Rotate and Scale on an opened Maya Binary
//! scene whose transform has rotate and scale pivots, and `inspect_scene` reporting the pivots.
//! Synthetic scene; the real Editor behind the real protocol.
use super::*;
use uuid::Uuid;

const T: [f64; 3] = [1.0, 2.0, 3.0];
const R: [f64; 3] = [10.0, 20.0, 30.0];
const S: [f64; 3] = [2.0, 3.0, 4.0];
const RP: [f64; 3] = [0.5, 0.25, -1.0];
const SP: [f64; 3] = [-1.0, 2.0, 0.5];

fn pivoted_bytes() -> Vec<u8> {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene, triple};
    let transform = [
        creation(b"object", None),
        triple(b"t", T),
        triple(b"r", R),
        triple(b"s", S),
        triple(b"rp", RP),
        triple(b"sp", SP),
    ]
    .concat();
    scene(
        &[
            group(b"XFRM", &transform, 4),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    )
    .to_vec()
}

struct Rig {
    editor: Editor,
    context: egui::Context,
    client: rmcp::service::RunningService<RoleClient, ()>,
    server: tokio::task::JoinHandle<()>,
    _directory: tempfile::TempDir,
    token: String,
    node: String,
    path: Vec<String>,
}

impl Rig {
    async fn open() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("pivoted.mb");
        std::fs::write(&source, pivoted_bytes()).unwrap();
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
            "document_applied"
        );
        let token = editor.scene_token.to_string();
        let scene = editor.history.scene();
        let id = scene.nodes()[0].id();
        let path = scene.transform_paths().unwrap().remove(0).path;
        let mut rig = Self {
            editor,
            context,
            client,
            server,
            _directory: directory,
            token,
            node: id.to_decimal(),
            path: path.iter().map(|id| id.to_decimal()).collect(),
        };
        let args = json!({"scene_token":rig.token,"node_id":rig.node,"instance_path":rig.path});
        rig.call("select_node", args).await;
        rig
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

    async fn try_call(
        &mut self,
        name: &'static str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        call_response(
            &mut self.editor,
            &self.context,
            self.client.peer(),
            name,
            arguments,
        )
        .await
    }

    fn begin_args(&self, rest: Value) -> Value {
        let mut args = json!({
            "scene_token":self.token,"node_id":self.node,"instance_path":self.path
        });
        args.as_object_mut()
            .unwrap()
            .extend(rest.as_object().unwrap().clone());
        args
    }

    async fn begin(&mut self, rest: Value) -> String {
        let args = self.begin_args(rest);
        let started = self
            .call("begin_manipulation", args)
            .await
            .structured_content
            .unwrap();
        assert_eq!(started["outcome"], "manipulation_started", "{started}");
        started["manipulation_id"].as_str().unwrap().to_owned()
    }

    async fn update(&mut self, id: &str, parameter: &str, value: Value) -> CallToolResult {
        let args = json!({"scene_token":self.token,"manipulation_id":id,parameter:value});
        self.call("update_manipulation", args).await
    }

    async fn finish(&mut self, id: &str, how: &str) -> CallToolResult {
        let args = json!({"scene_token":self.token,"manipulation_id":id,"finish":how});
        self.call("finish_manipulation", args).await
    }

    fn pivot_words(&self) -> ([u64; 3], [u64; 3]) {
        let node = &self.editor.history.scene().nodes()[0];
        (
            node.rotate_pivot().map(f64::to_bits),
            node.scale_pivot().map(f64::to_bits),
        )
    }

    fn channels(&self) -> [f64; 9] {
        self.editor.history.scene().nodes()[0].transform().values()
    }

    async fn end(self) {
        self.client.cancel().await.unwrap();
        drop(self.editor);
        self.server.await.unwrap();
    }
}

fn code(result: &CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn inspect_scene_reports_the_rotate_and_scale_pivots_of_the_opened_binary() {
    let mut rig = Rig::open().await;
    let inspection = rig
        .call("inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let attributes = &inspection["inspection"]["nodes"][0]["transform_attributes"];
    assert_eq!(attributes["rotate_pivot"], json!(RP));
    assert_eq!(attributes["scale_pivot"], json!(SP));
    assert_eq!(
        attributes["non_default"],
        json!(["rotate_pivot", "scale_pivot"])
    );
    rig.end().await;
}

#[tokio::test]
async fn move_rotate_and_scale_begin_update_and_commit_without_touching_the_pivots() {
    let mut rig = Rig::open().await;
    let pivots = rig.pivot_words();
    for (kind, handle, parameter, value, channel, expected) in [
        (
            "move",
            json!({"kind":"move","axis":"y"}),
            "distance",
            4.0,
            1,
            6.0,
        ),
        (
            "rotate",
            json!({"kind":"rotate","axis":"z","space":"object"}),
            "angle",
            15.0,
            5,
            f64::NAN,
        ),
        (
            "scale",
            json!({"kind":"scale","handle":"y","space":"object"}),
            "factor",
            2.0,
            7,
            6.0,
        ),
    ] {
        let channels_before = rig.channels();
        let id = rig.begin(handle).await;
        let updated = rig.update(&id, parameter, json!(value)).await;
        assert!(!updated.is_error.unwrap_or(false), "{kind}");
        // The binary r leaf is radians, and an object-space turn about Z recomposes all three
        // Euler channels, so Rotate is checked as "changed" and the others exactly.
        if expected.is_nan() {
            assert_ne!(rig.channels()[3..6], channels_before[3..6], "{kind}");
        } else {
            assert!(
                (rig.channels()[channel] - expected).abs() < 1e-9,
                "{kind} {:?}",
                rig.channels()
            );
        }
        let done = rig.finish(&id, "commit").await;
        assert!(!done.is_error.unwrap_or(false), "{kind}");
        assert_eq!(rig.pivot_words(), pivots, "{kind}");
    }
    rig.end().await;
}

#[tokio::test]
async fn a_cancelled_pivoted_rotate_restores_the_baseline() {
    let mut rig = Rig::open().await;
    let before = rig.editor.history.scene().clone();
    let id = rig
        .begin(json!({"kind":"rotate","axis":"x","space":"world"}))
        .await;
    rig.update(&id, "angle", json!(40.0)).await;
    assert_ne!(rig.editor.history.scene(), &before);
    rig.finish(&id, "cancel").await;
    assert_eq!(rig.editor.history.scene(), &before);
    rig.end().await;
}

#[tokio::test]
async fn the_guards_still_win_on_a_pivoted_scene() {
    let mut rig = Rig::open().await;
    // Stale token first.
    let mut args = rig.begin_args(json!({"kind":"rotate","axis":"x","space":"object"}));
    args["scene_token"] = json!(Uuid::new_v4().to_string());
    let stale = rig.call("begin_manipulation", args).await;
    assert_eq!(code(&stale), "stale_scene");
    // A text draft.
    rig.editor.channels[0] = "5".into();
    let args = rig.begin_args(json!({"kind":"scale","handle":"x","space":"object"}));
    let busy = rig.call("begin_manipulation", args).await;
    assert_eq!(code(&busy), "busy_editing");
    rig.editor.sync_fields();
    // A pending modal confirmation needs unsaved work: commit one Move first.
    let id = rig.begin(json!({"kind":"move","axis":"x"})).await;
    rig.update(&id, "distance", json!(1.0)).await;
    rig.finish(&id, "commit").await;
    let before = rig.editor.history.scene().clone();
    let token = rig.token.clone();
    rig.call("new_scene", json!({"scene_token":token})).await;
    let args = rig.begin_args(json!({"kind":"move","axis":"x"}));
    assert_eq!(
        code(&rig.call("begin_manipulation", args).await),
        "busy_editing"
    );
    assert_eq!(rig.editor.history.scene(), &before);
    rig.end().await;
}

#[tokio::test]
async fn a_second_begin_while_one_is_active_is_busy_and_a_close_request_waits() {
    let mut rig = Rig::open().await;
    let id = rig.begin(json!({"kind":"move","axis":"x"})).await;
    let args = rig.begin_args(json!({"kind":"rotate","axis":"x","space":"object"}));
    assert_eq!(
        code(&rig.call("begin_manipulation", args).await),
        "busy_manipulation"
    );
    let token = rig.token.clone();
    let close = rig
        .call("request_close", json!({"scene_token":token}))
        .await;
    assert_eq!(code(&close), "busy_manipulation");
    rig.finish(&id, "cancel").await;
    rig.end().await;
}

/// The protocol refuses these before any pivot code runs (the exactly-one rule and serde's f64
/// decoding): they show the shared update path rejects bad input on a pivoted scene and leaves
/// it alone, not that pivots are handled specially. Only 1e300 reaches the scene.
#[tokio::test]
async fn malformed_values_are_refused_by_the_protocol_and_leave_the_pivoted_scene_alone() {
    let mut rig = Rig::open().await;
    let id = rig
        .begin(json!({"kind":"rotate","axis":"y","space":"object"}))
        .await;
    let preview = rig.editor.history.scene().clone();
    // JSON has no NaN or infinity: a client's f64::NAN serialises to null.
    for (bad, message) in [
        (Value::Null, "give exactly one of distance, angle or factor"),
        (json!("NaN"), "expected f64"),
        (json!("Infinity"), "expected f64"),
        (json!([1.0, 2.0, 3.0]), "expected f64"),
        (json!({}), "expected f64"),
    ] {
        let args = json!({"scene_token":rig.token,"manipulation_id":id,"angle":bad});
        let Err(rmcp::ServiceError::McpError(error)) =
            rig.try_call("update_manipulation", args).await
        else {
            panic!("{bad}: expected a protocol error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS, "{bad}");
        // Our own message is pinned exactly; serde's wording only by the "expected f64" tail.
        if bad.is_null() {
            assert_eq!(error.message, message, "{bad}");
        } else {
            assert!(error.message.contains(message), "{bad}: {}", error.message);
        }
        assert_eq!(rig.editor.history.scene(), &preview, "{bad}");
    }
    // A huge finite angle reaches the scene, which refuses it as unrepresentable.
    let huge = rig.update(&id, "angle", json!(1e300)).await;
    assert_eq!(huge.is_error, Some(true));
    let error = &huge.structured_content.as_ref().unwrap()["error"];
    assert_eq!(error["code"], "manipulation_error");
    assert_eq!(
        error["message"],
        "Rotate unsupported: rotation cannot be represented accurately"
    );
    assert_eq!(rig.editor.history.scene(), &preview);
    rig.finish(&id, "cancel").await;
    rig.end().await;
}
