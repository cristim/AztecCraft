use super::*;

/// One component type under test; both tools share every guard and error path.
#[derive(Clone, Copy)]
struct Kind {
    noun: &'static str,
    tool: &'static str,
    index_field: &'static str,
    remembered_field: &'static str,
    mode: SelectionMode,
    count: usize,
}

const VERTEX: Kind = Kind {
    noun: "vertex",
    tool: "select_vertex",
    index_field: "vertex_index",
    remembered_field: "remembered_vertex",
    mode: SelectionMode::Vertex,
    count: 8,
};
const EDGE: Kind = Kind {
    noun: "edge",
    tool: "select_edge",
    index_field: "edge_index",
    remembered_field: "remembered_edge",
    mode: SelectionMode::Edge,
    count: 12,
};

impl Kind {
    fn remembered_index(self, editor: &Editor) -> Option<usize> {
        match self.mode {
            SelectionMode::Vertex => editor.remembered_vertex.as_ref().map(|t| t.vertex_index),
            SelectionMode::Edge => editor.remembered_edge.as_ref().map(|t| t.edge_index),
            other => unreachable!("{other:?} is not a component kind under test"),
        }
    }

    fn other_modes(self) -> impl Iterator<Item = &'static str> {
        ["object", "face", "hierarchy", "vertex", "edge"]
            .into_iter()
            .filter(move |mode| *mode != self.noun)
    }
}

async fn cube_with_target(
    kind: Kind,
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
) -> (String, String, Value) {
    let token = editor.scene_token.to_string();
    let created = call(
        editor,
        context,
        peer,
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = created["node_id"].as_str().unwrap().to_owned();
    let inspected = call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let node = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap();
    let mut path = node["instance_paths"][0].as_array().unwrap().clone();
    path.push(node["shape_ids"][0].clone());
    let mut target = json!({"node_id":id,"shape_id":node["shape_ids"][0],"instance_path":path});
    target[kind.index_field] = json!(7);
    (token, id, target)
}

type Baseline = (
    Option<azteccraft::scene::NodeId>,
    Vec<azteccraft::native_scene::NativeId>,
    Option<azteccraft::native_scene::FaceTarget>,
    Option<azteccraft::native_scene::VertexTarget>,
    Option<azteccraft::native_scene::EdgeTarget>,
    SelectionMode,
);

fn baseline(editor: &Editor) -> Baseline {
    (
        editor.selected(),
        editor.selected_path().to_vec(),
        editor.remembered_face.clone(),
        editor.remembered_vertex.clone(),
        editor.remembered_edge.clone(),
        editor.selection_mode,
    )
}

async fn set_mode(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    token: &str,
    mode: &str,
) -> CallToolResult {
    call(
        editor,
        context,
        peer,
        "set_selection_mode",
        json!({"scene_token":token,"mode":mode}),
    )
    .await
}

macro_rules! per_kind {
    ($body:ident, $vertex:ident, $edge:ident) => {
        #[tokio::test]
        async fn $vertex() {
            $body(VERTEX).await;
        }

        #[tokio::test]
        async fn $edge() {
            $body(EDGE).await;
        }
    };
}

per_kind!(
    mode_select_inspect_unchanged_clear_and_frame,
    sdk_vertex_mode_select_inspect_unchanged_clear_and_frame_share_editor_state,
    sdk_edge_mode_select_inspect_unchanged_clear_and_frame_share_editor_state
);
per_kind!(
    select_node_stays_exact,
    sdk_vertex_mode_select_node_stays_exact_and_clears_the_remembered_vertex,
    sdk_edge_mode_select_node_stays_exact_and_clears_the_remembered_edge
);
per_kind!(
    invalid_targets_and_wrong_modes,
    sdk_vertex_invalid_targets_and_wrong_modes_keep_the_baseline,
    sdk_edge_invalid_targets_and_wrong_modes_keep_the_baseline
);
per_kind!(
    occluded_and_off_screen,
    sdk_vertex_targets_may_name_occluded_and_off_screen_points,
    sdk_edge_targets_may_name_occluded_and_off_screen_edges
);
per_kind!(
    refuses_stale_token_draft_modal_and_active_move,
    sdk_vertex_selection_refuses_stale_token_draft_modal_and_active_move,
    sdk_edge_selection_refuses_stale_token_draft_modal_and_active_move
);

async fn mode_select_inspect_unchanged_clear_and_frame(kind: Kind) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, id, target) = cube_with_target(kind, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let tool = kind.tool;
    let mode = set_mode(&mut editor, &context, peer, &token, kind.noun).await;
    assert_eq!(mode.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.selection_mode, kind.mode);
    let args = json!({"scene_token":token,"target":target});
    let selected = call(&mut editor, &context, peer, tool, args.clone()).await;
    assert_eq!(selected.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(kind.remembered_index(&editor), Some(7));
    assert_eq!(editor.remembered_face, None);
    let repeat = call(&mut editor, &context, peer, tool, args).await;
    assert_eq!(repeat.structured_content.unwrap()["outcome"], "unchanged");
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    let history_len = editor.history.can_undo();
    for mode in [kind.noun, "object", "face", kind.noun] {
        set_mode(&mut editor, &context, peer, &token, mode).await;
        let inspected = call(&mut editor, &context, peer, "inspect_scene", json!({}))
            .await
            .structured_content
            .unwrap();
        assert_eq!(inspected["inspection"]["selection_mode"], mode);
        assert_eq!(inspected["inspection"][kind.remembered_field], target);
        for other in ["remembered_face", "remembered_vertex", "remembered_edge"] {
            if other != kind.remembered_field {
                assert!(inspected["inspection"][other].is_null(), "{other}");
            }
        }
        assert_eq!(inspected["inspection"]["selected_node"], id);
    }
    let camera = editor.camera.clone();
    let framed = call(
        &mut editor,
        &context,
        peer,
        "frame_selected",
        json!({"scene_token":token}),
    )
    .await;
    assert_ne!(framed.is_error, Some(true));
    assert_ne!(editor.camera, camera);
    let cleared = call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":token,"target":null}),
    )
    .await;
    assert_eq!(cleared.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(kind.remembered_index(&editor), None);
    assert_eq!(editor.selected().unwrap().to_decimal(), id);
    let no_target = call(
        &mut editor,
        &context,
        peer,
        "frame_selected",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(no_target.is_error, Some(true));
    assert_eq!(
        no_target.structured_content.unwrap()["error"]["code"],
        "no_selection"
    );
    let state = call(&mut editor, &context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    assert!(state["inspection"][kind.remembered_field].is_null());
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.can_undo(), history_len);
    assert_eq!(editor.scene_token.to_string(), token);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn select_node_stays_exact(kind: Kind) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, id, target) = cube_with_target(kind, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let plane = call(
        &mut editor,
        &context,
        peer,
        "create_primitive",
        json!({"scene_token":token,"primitive":"plane"}),
    )
    .await
    .structured_content
    .unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (tool, args) in [
        (
            "set_selection_mode",
            json!({"scene_token":token,"mode":kind.noun}),
        ),
        ("select_node", json!({"scene_token":token,"node_id":id})),
        (kind.tool, json!({"scene_token":token,"target":target})),
    ] {
        assert_ne!(
            call(&mut editor, &context, peer, tool, args).await.is_error,
            Some(true)
        );
    }
    assert!(kind.remembered_index(&editor).is_some());
    let other = call(
        &mut editor,
        &context,
        peer,
        "select_node",
        json!({"scene_token":token,"node_id":plane}),
    )
    .await;
    assert_ne!(other.is_error, Some(true));
    assert_eq!(editor.selected().unwrap().to_decimal(), plane);
    assert_eq!(editor.selection_mode, kind.mode);
    assert_eq!(kind.remembered_index(&editor), None);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn invalid_targets_and_wrong_modes(kind: Kind) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, id, target) = cube_with_target(kind, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let tool = kind.tool;
    let index_field = kind.index_field;
    // Every other mode: even null is refused and nothing changes.
    for mode in kind.other_modes() {
        set_mode(&mut editor, &context, peer, &token, mode).await;
        let before = baseline(&editor);
        for value in [target.clone(), Value::Null] {
            let refused = call(
                &mut editor,
                &context,
                peer,
                tool,
                json!({"scene_token":token,"target":value}),
            )
            .await;
            assert_eq!(refused.is_error, Some(true), "{mode}");
            assert_eq!(
                refused.structured_content.unwrap()["error"]["code"],
                "invalid_input",
                "{mode}"
            );
            assert_eq!(baseline(&editor), before, "{mode}");
        }
    }
    set_mode(&mut editor, &context, peer, &token, kind.noun).await;
    call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":token,"target":target}),
    )
    .await;
    let other = call(
        &mut editor,
        &context,
        peer,
        "create_primitive",
        json!({"scene_token":token,"primitive":"plane"}),
    )
    .await
    .structured_content
    .unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &mut editor,
        &context,
        peer,
        "select_node",
        json!({"scene_token":token,"node_id":id}),
    )
    .await;
    call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":token,"target":target}),
    )
    .await;
    let before = baseline(&editor);
    assert!(kind.remembered_index(&editor).is_some());
    let before_scene = editor.history.scene().clone();
    let before_dirty = editor.is_dirty();
    for field in ["node_id", "shape_id", "instance_path", index_field] {
        let mut invalid = target.clone();
        invalid[field] = match field {
            "node_id" => json!(other),
            "shape_id" => json!("18446744073709551615"),
            "instance_path" => json!(["18446744073709551615", target["shape_id"]]),
            _ => json!(usize::MAX),
        };
        let result = call(
            &mut editor,
            &context,
            peer,
            tool,
            json!({"scene_token":token,"target":invalid}),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "{field}");
        assert_eq!(baseline(&editor), before, "{field}");
    }
    let mut one_past = target.clone();
    one_past[index_field] = json!(kind.count);
    let rejected = call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":token,"target":one_past}),
    )
    .await;
    assert_eq!(rejected.is_error, Some(true));
    assert_eq!(baseline(&editor), before);
    let target_with = |field: &str, value: Value| {
        let mut object = json!({"node_id":id,"shape_id":target["shape_id"],"instance_path":target["instance_path"]});
        object[field] = value;
        object
    };
    let mut short_path = target_with(index_field, json!(0));
    short_path["instance_path"] = json!(["1"]);
    for malformed in [
        json!({"scene_token":token}),
        json!({"scene_token":token,"target":target_with(index_field, json!(-1))}),
        json!({"scene_token":token,"target":target_with(index_field, json!(0.5))}),
        json!({"scene_token":token,"target":short_path}),
        json!({"scene_token":token,"target":target_with("face_index", json!(0))}),
    ] {
        let result = call_response(&mut editor, &context, peer, tool, malformed).await;
        assert!(matches!(result, Err(rmcp::ServiceError::McpError(_))));
        assert_eq!(baseline(&editor), before);
    }
    assert_eq!(editor.history.scene(), &before_scene);
    assert_eq!(editor.is_dirty(), before_dirty);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn occluded_and_off_screen(kind: Kind) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, _, target) = cube_with_target(kind, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let tool = kind.tool;
    set_mode(&mut editor, &context, peer, &token, kind.noun).await;
    // Part of the cube is hidden behind itself from the default camera (a review
    // measured edges 0, 3 and 8, the edges incident to vertex 0, as fully hidden),
    // so the loop includes fully occluded entries; every index is accepted whether
    // or not it is visible.
    for index in 0..kind.count {
        let mut explicit = target.clone();
        explicit[kind.index_field] = json!(index);
        let result = call(
            &mut editor,
            &context,
            peer,
            tool,
            json!({"scene_token":token,"target":explicit}),
        )
        .await;
        assert_ne!(result.is_error, Some(true), "{index}");
        assert_eq!(kind.remembered_index(&editor), Some(index));
    }
    let panned = call(
        &mut editor,
        &context,
        peer,
        "pan_camera",
        json!({"scene_token":token,"world_units":[1000.0,1000.0]}),
    )
    .await;
    assert_ne!(panned.is_error, Some(true));
    assert_eq!(panned.structured_content.unwrap()["outcome"], "changed");
    let result = call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":token,"target":target}),
    )
    .await;
    assert_ne!(result.is_error, Some(true));
    assert_eq!(kind.remembered_index(&editor), Some(7));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn refuses_stale_token_draft_modal_and_active_move(kind: Kind) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, _, target) = cube_with_target(kind, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let tool = kind.tool;
    set_mode(&mut editor, &context, peer, &token, kind.noun).await;
    let before = baseline(&editor);
    let scene = editor.history.scene().clone();
    let args = json!({"scene_token":token,"target":target});
    let code = |result: CallToolResult| result.structured_content.unwrap()["error"]["code"].clone();

    let stale = call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":Uuid::nil().to_string(),"target":target}),
    )
    .await;
    assert_eq!(code(stale), "stale_scene");
    assert_eq!(baseline(&editor), before);

    editor.name = "human_draft".into();
    let draft = call(&mut editor, &context, peer, tool, args.clone()).await;
    assert_eq!(code(draft), "busy_editing");
    assert_eq!(editor.name, "human_draft");
    assert_eq!(baseline(&editor), before);
    editor.sync_fields();

    editor.request_close();
    let modal = call(&mut editor, &context, peer, tool, args.clone()).await;
    assert_eq!(code(modal), "busy_editing");
    assert_eq!(baseline(&editor), before);
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();

    let (mut editor, client, server) = start(&context).await;
    let (token, id, target) = cube_with_target(kind, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    // Move refuses component modes, so it starts in Object mode; the busy guard
    // must still win over the wrong-mode validation.
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        json!({"scene_token":token,"node_id":id,"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    let before = baseline(&editor);
    let preview = editor.history.scene().clone();
    let busy = call(
        &mut editor,
        &context,
        peer,
        tool,
        json!({"scene_token":token,"target":target}),
    )
    .await;
    assert_eq!(code(busy), "busy_manipulation");
    assert_eq!(baseline(&editor), before);
    assert_eq!(editor.history.scene(), &preview);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_begin_manipulation_refuses_every_kind_in_vertex_and_edge_modes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, id, _) = cube_with_target(VERTEX, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    for (mode, noun) in [("vertex", "Vertex"), ("edge", "Edge")] {
        set_mode(&mut editor, &context, peer, &token, mode).await;
        for (kind, name, extra) in [
            ("move", "Move", json!({"axis":"x"})),
            ("rotate", "Rotate", json!({"axis":"y","space":"object"})),
            ("scale", "Scale", json!({"handle":"x","space":"object"})),
        ] {
            let before = baseline(&editor);
            let dirty = editor.is_dirty();
            let scene = editor.history.scene().clone();
            let mut arguments = json!({
                "scene_token":token,"node_id":id,"instance_path":path,"kind":kind
            });
            arguments
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let refused = call(&mut editor, &context, peer, "begin_manipulation", arguments).await;
            assert_eq!(refused.is_error, Some(true), "{mode} {kind}");
            let error = &refused.structured_content.unwrap()["error"];
            assert_eq!(error["code"], "invalid_input", "{mode} {kind}");
            assert_eq!(
                error["message"],
                format!("{name} is unavailable in {noun} selection; switch to Object selection"),
                "{mode} {kind}"
            );
            assert_eq!(baseline(&editor), before, "{mode} {kind}");
            assert_eq!(editor.is_dirty(), dirty, "{mode} {kind}");
            assert_eq!(editor.scene_token.to_string(), token, "{mode} {kind}");
            assert_eq!(editor.history.scene(), &scene, "{mode} {kind}");
            assert!(editor.history.active_manipulation().is_none());
        }
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_each_component_type_keeps_its_own_remembered_pick() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (token, _, vertex) = cube_with_target(VERTEX, &mut editor, &context, client.peer()).await;
    let peer = client.peer();
    let mut edge = vertex.clone();
    edge.as_object_mut().unwrap().remove("vertex_index");
    edge["edge_index"] = json!(3);
    set_mode(&mut editor, &context, peer, &token, "vertex").await;
    call(
        &mut editor,
        &context,
        peer,
        "select_vertex",
        json!({"scene_token":token,"target":vertex}),
    )
    .await;
    set_mode(&mut editor, &context, peer, &token, "edge").await;
    let selected = call(
        &mut editor,
        &context,
        peer,
        "select_edge",
        json!({"scene_token":token,"target":edge}),
    )
    .await;
    assert_eq!(selected.structured_content.unwrap()["outcome"], "changed");
    // The vertex pick survives while Edge mode is active.
    assert_eq!(VERTEX.remembered_index(&editor), Some(7));
    assert_eq!(EDGE.remembered_index(&editor), Some(3));
    let inspected = call(&mut editor, &context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    assert_eq!(inspected["inspection"]["remembered_vertex"], vertex);
    assert_eq!(inspected["inspection"]["remembered_edge"], edge);
    // Back in Vertex mode the vertex pick is the active one again.
    set_mode(&mut editor, &context, peer, &token, "vertex").await;
    let framed = call(
        &mut editor,
        &context,
        peer,
        "frame_selected",
        json!({"scene_token":token}),
    )
    .await;
    assert_ne!(framed.is_error, Some(true));
    call(
        &mut editor,
        &context,
        peer,
        "select_vertex",
        json!({"scene_token":token,"target":null}),
    )
    .await;
    let none = call(
        &mut editor,
        &context,
        peer,
        "frame_selected",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(
        none.structured_content.unwrap()["error"]["code"],
        "no_selection"
    );
    assert_eq!(EDGE.remembered_index(&editor), Some(3));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
