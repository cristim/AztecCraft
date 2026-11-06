use super::*;
use crate::editor_operations::SidebarPanel::{self, AttributeEditor as AE, ChannelBox as CB};

fn panel_name(panel: SidebarPanel) -> &'static str {
    match panel {
        CB => "channel_box",
        AE => "attribute_editor",
    }
}

async fn inspect(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

fn dock_of(inspection: &Value) -> Option<SidebarPanel> {
    let panels = &inspection["sidebar_panels"];
    let channel_box = panels["channel_box"].as_bool().unwrap();
    let attribute_editor = panels["attribute_editor"].as_bool().unwrap();
    assert!(!(channel_box && attribute_editor), "at most one dock panel");
    match (channel_box, attribute_editor) {
        (true, _) => Some(CB),
        (_, true) => Some(AE),
        _ => None,
    }
}

async fn set_panel(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    panel: SidebarPanel,
    visible: bool,
) -> Value {
    let token = editor.scene_token.to_string();
    call(
        editor,
        context,
        client.peer(),
        "set_sidebar_panel_visibility",
        json!({"scene_token": token, "panel": panel_name(panel), "visible": visible}),
    )
    .await
    .structured_content
    .unwrap()
}

async fn select_tab(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    token: &str,
    tab: Value,
) -> Value {
    call(
        editor,
        context,
        client.peer(),
        "select_attribute_editor_tab",
        json!({"scene_token": token, "tab": tab}),
    )
    .await
    .structured_content
    .unwrap()
}

#[tokio::test]
async fn sdk_transition_table_every_cell() {
    let ops = [(CB, true), (CB, false), (AE, true), (AE, false)];
    type Cell = (Option<SidebarPanel>, &'static str);
    let table: [(Option<SidebarPanel>, [Cell; 4]); 3] = [
        (
            Some(CB),
            [
                (Some(CB), "unchanged"),
                (None, "changed"),
                (Some(AE), "changed"),
                (Some(CB), "unchanged"),
            ],
        ),
        (
            Some(AE),
            [
                (Some(CB), "changed"),
                (Some(AE), "unchanged"),
                (Some(AE), "unchanged"),
                (None, "changed"),
            ],
        ),
        (
            None,
            [
                (Some(CB), "changed"),
                (None, "unchanged"),
                (Some(AE), "changed"),
                (None, "unchanged"),
            ],
        ),
    ];
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let scene = editor.history.scene().clone();
    for (current, cells) in table {
        for ((panel, visible), (expected, outcome)) in ops.into_iter().zip(cells) {
            editor.sidebar_dock = current;
            let before = inspect(&mut editor, &context, &client).await;
            assert_eq!(dock_of(&before), current);
            let result = set_panel(&mut editor, &context, &client, panel, visible).await;
            let cell = format!("{current:?} set({panel:?},{visible})");
            assert_eq!(result["outcome"], outcome, "{cell}");
            let after = inspect(&mut editor, &context, &client).await;
            assert_eq!(dock_of(&after), expected, "{cell}");
            assert_eq!(editor.sidebar_dock, expected, "{cell}");
            assert_eq!(editor.history.scene(), &scene, "{cell}");
            assert_eq!(
                after["attribute_editor_tab"].is_null(),
                expected != Some(AE)
            );
        }
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_tab_selection_resolves_reports_and_refuses() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    // Two creations so no id in this test is the trivial "1".
    editor.create(Primitive::Cube);
    editor.create(Primitive::Plane);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let node = editor.selected().unwrap().to_decimal();
    assert_ne!(node, "1");
    let shape = editor.history.scene().shapes()[1].id().to_decimal();
    assert_ne!(shape, "1");
    let transform_tab = json!({"kind": "transform", "id": node});
    let shape_tab = json!({"kind": "shape", "id": shape});

    // Hidden panel: refused as invalid_input, nothing stored.
    let before = inspect(&mut editor, &context, &client).await;
    let hidden = select_tab(&mut editor, &context, &client, &token, shape_tab.clone()).await;
    assert_eq!(hidden["error"]["code"], "invalid_input");
    assert!(
        hidden["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Attribute Editor")
    );
    assert_eq!(editor.attribute_editor_tab, None);
    assert_eq!(inspect(&mut editor, &context, &client).await, before);

    set_panel(&mut editor, &context, &client, AE, true).await;
    let shown = inspect(&mut editor, &context, &client).await;
    assert_eq!(shown["attribute_editor_tab"], transform_tab);
    let first = select_tab(&mut editor, &context, &client, &token, shape_tab.clone()).await;
    assert_eq!(first["outcome"], "changed");
    let repeat = select_tab(&mut editor, &context, &client, &token, shape_tab.clone()).await;
    assert_eq!(repeat["outcome"], "unchanged");
    assert_eq!(
        inspect(&mut editor, &context, &client).await["attribute_editor_tab"],
        shape_tab
    );
    let back = select_tab(
        &mut editor,
        &context,
        &client,
        &token,
        transform_tab.clone(),
    )
    .await;
    assert_eq!(back["outcome"], "changed");

    // A failed call leaves state unchanged: unknown shape, foreign node, cube has no material.
    let state = inspect(&mut editor, &context, &client).await;
    for tab in [
        json!({"kind": "shape", "id": "999999"}),
        json!({"kind": "transform", "id": "999999"}),
        json!({"kind": "material", "id": shape}),
        json!({"kind": "shape", "id": node}),
    ] {
        let refused = select_tab(&mut editor, &context, &client, &token, tab.clone()).await;
        assert_eq!(refused["error"]["code"], "invalid_input", "{tab}");
        assert_eq!(
            inspect(&mut editor, &context, &client).await,
            state,
            "{tab}"
        );
    }

    // Selection change resets to the transform tab.
    select_tab(&mut editor, &context, &client, &token, shape_tab).await;
    editor.create(Primitive::Plane);
    frame(&mut editor, &context, vec![]);
    let shown = inspect(&mut editor, &context, &client).await;
    assert_eq!(shown["attribute_editor_tab"]["kind"], "transform");
    assert_eq!(
        shown["attribute_editor_tab"]["id"].as_str(),
        shown["selected_node"].as_str()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_material_tab_selects_and_drives_the_material_shape() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("material.mb");
    std::fs::write(&path, appearance_fixture::appearance_specimen_with_users(2)).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let before_open = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token": before_open, "path": path}),
    )
    .await;
    let token = editor.scene_token.to_string();
    let id = editor.history.scene().nodes()[0].id();
    editor.select(Some(id));
    let shape = editor.history.scene().shapes()[0].id();
    set_panel(&mut editor, &context, &client, AE, true).await;
    let tab = json!({"kind": "material", "id": shape.to_decimal()});
    let result = select_tab(&mut editor, &context, &client, &token, tab.clone()).await;
    assert_eq!(result["outcome"], "changed");
    assert_eq!(editor.material_shape, Some(shape));
    assert_eq!(
        inspect(&mut editor, &context, &client).await["attribute_editor_tab"],
        tab
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_attribute_editor_operations_are_guarded_like_the_other_view_setters() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.sidebar_dock = Some(AE);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let node = editor.selected().unwrap().to_decimal();
    let tab = json!({"kind": "transform", "id": node});
    let scene = editor.history.scene().clone();
    let baseline = inspect(&mut editor, &context, &client).await;

    // Stale token.
    let stale = Uuid::new_v4().to_string();
    for result in [
        select_tab(&mut editor, &context, &client, &stale, tab.clone()).await,
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_sidebar_panel_visibility",
            json!({"scene_token": stale, "panel": "attribute_editor", "visible": false}),
        )
        .await
        .structured_content
        .unwrap(),
    ] {
        assert_eq!(result["error"]["code"], "stale_scene");
    }
    assert_eq!(inspect(&mut editor, &context, &client).await, baseline);

    // Human draft: any draft is busy_editing, valid or not.
    for draft in ["2", "invalid"] {
        editor.channels[0] = draft.into();
        for result in [
            select_tab(&mut editor, &context, &client, &token, tab.clone()).await,
            call(
                &mut editor,
                &context,
                client.peer(),
                "set_sidebar_panel_visibility",
                json!({"scene_token": token, "panel": "channel_box", "visible": true}),
            )
            .await
            .structured_content
            .unwrap(),
        ] {
            assert_eq!(result["error"]["code"], "busy_editing", "{draft}");
        }
        assert_eq!(editor.channels[0], draft);
        assert_eq!(editor.sidebar_dock, Some(AE));
        assert_eq!(editor.history.scene(), &scene);
    }
    editor.channels = Default::default();
    editor.sync_fields();

    // Modal and closing.
    editor.request_close();
    for result in [
        select_tab(&mut editor, &context, &client, &token, tab.clone()).await,
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_sidebar_panel_visibility",
            json!({"scene_token": token, "panel": "attribute_editor", "visible": false}),
        )
        .await
        .structured_content
        .unwrap(),
    ] {
        assert_eq!(result["error"]["code"], "busy_editing");
    }
    assert_eq!(editor.sidebar_dock, Some(AE));
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_attribute_editor_operations_are_busy_during_an_active_move() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token": token, "primitive": "cube"}),
    )
    .await
    .structured_content
    .unwrap();
    editor.sidebar_dock = Some(AE);
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token": token, "node_id": created["node_id"], "instance_path": path, "kind": "move", "axis": "x"}),
    )
    .await;
    let preview = editor.history.scene().clone();
    let tab = json!({"kind": "transform", "id": created["node_id"]});
    for result in [
        select_tab(&mut editor, &context, &client, &token, tab).await,
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_sidebar_panel_visibility",
            json!({"scene_token": token, "panel": "channel_box", "visible": true}),
        )
        .await
        .structured_content
        .unwrap(),
    ] {
        assert_eq!(result["error"]["code"], "busy_manipulation");
    }
    assert_eq!(editor.sidebar_dock, Some(AE));
    assert_eq!(editor.attribute_editor_tab, None);
    assert_eq!(editor.history.scene(), &preview);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_shape_tab_beyond_the_cap_is_refused_as_invalid_input() {
    use crate::app::features::attribute_editor::MAX_SHAPE_TABS;
    let mut children = binary_fixture::group(
        b"XFRM",
        &[
            binary_fixture::creation(b"object", None),
            binary_fixture::triple(b"t", [0.0, 0.0, 0.0]),
        ]
        .concat(),
        4,
    );
    for index in 0..MAX_SHAPE_TABS + 2 {
        children.extend(binary_fixture::group(
            b"DMSH",
            &[
                binary_fixture::creation(format!("shape{index}").as_bytes(), Some(b"object")),
                binary_fixture::chunk(
                    b"MESH",
                    &binary_fixture::mesh_payload(&[0, 1, 0x6000_0002]),
                    8,
                ),
            ]
            .concat(),
            4,
        ));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("many.mb");
    std::fs::write(
        &path,
        binary_fixture::scene_with_units(&children, b"cm", b"deg"),
    )
    .unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let before_open = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token": before_open, "path": path}),
    )
    .await;
    let token = editor.scene_token.to_string();
    let node = editor.history.scene().nodes()[0].id();
    editor.select(Some(node));
    set_panel(&mut editor, &context, &client, AE, true).await;
    let transform = editor.history.scene().nodes()[0].transform_id();
    let shapes: Vec<_> = editor.direct_shapes(transform).collect();
    assert_eq!(shapes.len(), MAX_SHAPE_TABS + 2);
    let last_allowed = shapes[MAX_SHAPE_TABS - 1].to_decimal();
    let ok = select_tab(
        &mut editor,
        &context,
        &client,
        &token,
        json!({"kind": "shape", "id": last_allowed}),
    )
    .await;
    assert_eq!(ok["outcome"], "changed");
    let state = inspect(&mut editor, &context, &client).await;
    for kind in ["shape", "material"] {
        let refused = select_tab(
            &mut editor,
            &context,
            &client,
            &token,
            json!({"kind": kind, "id": shapes[MAX_SHAPE_TABS].to_decimal()}),
        )
        .await;
        assert_eq!(refused["error"]["code"], "invalid_input", "{kind}");
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains(&MAX_SHAPE_TABS.to_string()),
            "{refused}"
        );
        assert_eq!(inspect(&mut editor, &context, &client).await, state);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

fn extras_scene(units: &str, attributes: &str) -> azteccraft::native_scene::NativeScene {
    let body = format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\";\ncurrentUnit {units} -t film;\n\
         createNode transform -n \"a\"; {attributes}\ncreateNode transform -n \"b\";\n"
    );
    azteccraft::native_scene::ascii::decode(body.into_bytes().into()).unwrap()
}

#[tokio::test]
async fn sdk_inspection_reports_transform_extras_with_units_and_non_default_names() {
    // Maya ASCII only opens centimeter/degree scenes, so radian and meter units are covered by the
    // core test `transform_attributes_report_names_degrees_and_declared_linear_units`.
    let cases = [(
        "-l centimeter -a degree",
        "setAttr \".ro\" 1; setAttr \".ra\" -type \"double3\" 10 20 -30; \
             setAttr \".sh\" -type \"double3\" 0.5 0 0.25; \
             setAttr \".rpt\" -type \"double3\" 1 2 3; setAttr \".spt\" -type \"double3\" -1 0 2;",
        "yzx",
        [10.0, 20.0, -30.0],
        [1.0, 2.0, 3.0],
    )];
    for (units, attributes, order, axis, rpt) in cases {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.history = azteccraft::history::History::new(extras_scene(units, attributes));
        let inspection = inspect(&mut editor, &context, &client).await;
        let first = &inspection["nodes"][0]["transform_attributes"];
        assert_eq!(first["rotate_order"], order, "{units}");
        // Degree scenes report the declared value unchanged.
        assert_eq!(
            first["rotate_axis_degrees"],
            json!(axis),
            "{units}: {first}"
        );
        assert_eq!(first["shear"], json!([0.5, 0.0, 0.25]));
        assert_eq!(first["rotate_pivot_translate"], json!(rpt));
        assert_eq!(first["scale_pivot_translate"], json!([-1.0, 0.0, 2.0]));
        assert_eq!(
            first["non_default"],
            json!([
                "rotate_order",
                "rotate_axis",
                "shear",
                "rotate_pivot_translate",
                "scale_pivot_translate"
            ])
        );
        // A default node omits the key entirely; the extras node carries it.
        assert!(inspection["nodes"][1].get("transform_attributes").is_none());
        assert!(inspection["nodes"][0].get("transform_attributes").is_some());
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
