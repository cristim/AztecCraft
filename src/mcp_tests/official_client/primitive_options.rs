use super::*;

/// MCP primitive, options, then expected points, edges and faces (closed forms: Euler 2).
const BUILDS: [(&str, &str, [u64; 3]); 21] = [
    (
        "cube",
        r#"{"width":2,"height":3,"depth":4,"subdivisions_width":2,"subdivisions_height":3,"subdivisions_depth":4,"axis":"x"}"#,
        [54, 104, 52],
    ),
    (
        "plane",
        r#"{"width":5,"height":0.5,"subdivisions_width":3,"subdivisions_height":4,"axis":"z"}"#,
        [20, 31, 12],
    ),
    (
        "sphere",
        r#"{"radius":2.5,"subdivisions_axis":8,"subdivisions_height":5,"axis":"z"}"#,
        [34, 72, 40],
    ),
    (
        "cylinder",
        r#"{"radius":0.25,"height":10,"subdivisions_axis":6,"subdivisions_height":3,"axis":"x"}"#,
        [24, 42, 20],
    ),
    (
        "cone",
        r#"{"radius":3,"height":1e6,"subdivisions_axis":6,"subdivisions_height":3,"axis":"y"}"#,
        [19, 36, 19],
    ),
    (
        "torus",
        r#"{"radius":3,"section_radius":4,"subdivisions_axis":6,"subdivisions_height":5,"axis":"x"}"#,
        [30, 60, 30],
    ),
    // Cap subdivisions add a centre vertex and axis * caps faces per cap (cone: base only).
    (
        "cylinder",
        r#"{"subdivisions_axis":6,"subdivisions_height":3,"subdivisions_caps":2}"#,
        [38, 78, 42],
    ),
    (
        "cone",
        r#"{"subdivisions_axis":6,"subdivisions_height":3,"subdivisions_caps":2,"round_cap":true}"#,
        [26, 54, 30],
    ),
    // Round cap alone is accepted and changes nothing while caps are 0.
    ("cylinder", r#"{"round_cap":true}"#, [40, 60, 22]),
    (
        "cone",
        r#"{"round_cap":true,"subdivisions_caps":0}"#,
        [21, 40, 21],
    ),
    // Platonic solids: every solid, by name, with a radius and an axis.
    (
        "platonic_solid",
        r#"{"solid":"dodecahedron","radius":2,"axis":"x"}"#,
        [20, 30, 12],
    ),
    (
        "platonic_solid",
        r#"{"solid":"icosahedron","radius":0.5}"#,
        [12, 30, 20],
    ),
    ("platonic_solid", r#"{"solid":"octahedron"}"#, [6, 12, 8]),
    (
        "platonic_solid",
        r#"{"solid":"tetrahedron","axis":"z"}"#,
        [4, 6, 4],
    ),
    (
        "platonic_solid",
        r#"{"solid":"cube","radius":1e6}"#,
        [8, 12, 6],
    ),
    // Pyramid (n+1 points, 2n edges, n+1 faces) and prism (2n, 3n, n+2).
    (
        "pyramid",
        r#"{"side_length":2,"number_of_sides":5,"axis":"x"}"#,
        [6, 10, 6],
    ),
    ("pyramid", r#"{"number_of_sides":3}"#, [4, 6, 4]),
    ("pyramid", r#"{"number_of_sides":128}"#, [129, 256, 129]),
    (
        "prism",
        r#"{"side_length":1,"length":4,"number_of_sides":6,"axis":"z"}"#,
        [12, 18, 8],
    ),
    (
        "prism",
        r#"{"number_of_sides":128,"length":1e6}"#,
        [256, 384, 130],
    ),
    // An empty options object is the defaults, like omitting it.
    ("cube", "{}", [8, 12, 6]),
];

fn counts(inspected: &Value) -> Vec<[u64; 3]> {
    inspected["inspection"]["shapes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|shape| ["points", "edges", "faces"].map(|key| shape[key].as_u64().unwrap()))
        .collect()
}

async fn inspect(editor: &mut Editor, context: &egui::Context, client: &Peer<RoleClient>) -> Value {
    call(editor, context, client, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn optioned_create_builds_the_requested_mesh_and_undoes_through_the_shared_editor() {
    for (primitive, options, expected) in BUILDS {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let options: Value = serde_json::from_str(options).unwrap();
        let before = editor.history.scene().clone();
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":options}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(created["outcome"], "created", "{primitive}");
        assert_eq!(
            counts(&inspect(&mut editor, &context, client.peer()).await),
            vec![expected],
            "{primitive} {options}"
        );
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &before);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn calls_without_options_keep_the_default_meshes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    for (primitive, expected) in [
        ("cube", [8, 12, 6]),
        ("plane", [121, 220, 100]),
        ("sphere", [382, 780, 400]),
        ("cylinder", [40, 60, 22]),
        ("cone", [21, 40, 21]),
        ("torus", [400, 800, 400]),
        ("platonic_solid", [20, 30, 12]),
        ("pyramid", [5, 8, 5]),
        ("prism", [6, 9, 5]),
    ] {
        let reply = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive}),
        )
        .await;
        assert_eq!(reply.is_error, Some(false), "{primitive}");
        let shapes = counts(&inspect(&mut editor, &context, client.peer()).await);
        assert_eq!(shapes.last(), Some(&expected), "{primitive}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_or_mismatched_options_are_invalid_params_and_leave_the_editor_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let selected = (editor.selected(), editor.selected_path().to_vec());
    let cases = [
        ("cube", json!(null)),
        ("cube", json!([])),
        ("cube", json!({"width":null})),
        ("cube", json!({"axis":null})),
        ("cube", json!({"width":0})),
        ("cube", json!({"width":-1})),
        ("cube", json!({"width":1e-7})),
        ("cube", json!({"width":1000001})),
        ("cube", json!({"width":"2"})),
        ("cube", json!({"axis":"w"})),
        ("cube", json!({"axis":"X"})),
        ("cube", json!({"subdivisions_width":0})),
        ("cube", json!({"subdivisions_width":129})),
        ("cube", json!({"subdivisions_width":3.0})),
        ("cube", json!({"subdivisions_width":2.5})),
        ("cube", json!({"subdivisions_width":-1})),
        ("cube", json!({"subdivisions_width":4294967296u64})),
        ("cube", json!({"subdivisions_width":"3"})),
        // Over the face budget although every field is in range.
        (
            "cube",
            json!({"subdivisions_width":128,"subdivisions_height":128,"subdivisions_depth":128}),
        ),
        ("cube", json!({"depht":1})),
        ("cube", json!({"radius":1})),
        ("plane", json!({"depth":1})),
        ("plane", json!({"subdivisions_depth":2})),
        ("sphere", json!({"width":1})),
        ("sphere", json!({"height":1})),
        ("sphere", json!({"subdivisions_axis":2})),
        ("sphere", json!({"subdivisions_height":1})),
        ("cylinder", json!({"width":1})),
        ("cylinder", json!({"subdivisions_axis":2})),
        ("cylinder", json!({"subdivisions_height":0})),
        ("cone", json!({"depth":1})),
        ("cone", json!({"radius":0})),
        ("cone", json!({"subdivisions_height":129})),
        ("torus", json!({"height":1})),
        ("torus", json!({"section_radius":0})),
        ("torus", json!({"radius":null})),
        ("torus", json!({"subdivisions_axis":2})),
        ("torus", json!({"subdivisions_height":2})),
        ("torus", json!({"subdivisions_height":129})),
        ("torus", json!({"subdivisions_axis":2.5})),
        ("torus", json!({"twist":null})),
        ("torus", json!({"twist":"45"})),
        ("cylinder", json!({"subdivisions_caps":129})),
        ("cone", json!({"subdivisions_caps":-1})),
        ("cylinder", json!({"subdivisions_caps":"2"})),
        ("cone", json!({"subdivisions_caps":2.5})),
        ("cylinder", json!({"subdivisions_caps":null})),
        ("cylinder", json!({"round_cap":"true"})),
        ("cone", json!({"round_cap":1})),
        ("cone", json!({"round_cap":null})),
        // The cone's Maya flag is -subdivisionsCap, but the wire field is subdivisions_caps only.
        ("cone", json!({"subdivisions_cap":1})),
        ("cube", json!({"subdivisions_caps":1})),
        ("sphere", json!({"round_cap":true})),
        ("torus", json!({"subdivisions_caps":1})),
        // 2 caps of 128 x 128 faces exceed the 20,000-face budget though every field is in range.
        (
            "cylinder",
            json!({"subdivisions_axis":128,"subdivisions_caps":128}),
        ),
        ("cylinder", json!({"twist":45})),
        ("sphere", json!({"twist":0})),
        ("platonic_solid", json!({"solid":"cone"})),
        ("platonic_solid", json!({"solid":"Cube"})),
        ("platonic_solid", json!({"solid":null})),
        ("platonic_solid", json!({"solid":3})),
        ("platonic_solid", json!({"radius":0})),
        ("platonic_solid", json!({"radius":-1})),
        ("platonic_solid", json!({"radius":1000001})),
        ("platonic_solid", json!({"radius":"1"})),
        ("platonic_solid", json!({"radius":null})),
        ("platonic_solid", json!({"axis":"w"})),
        ("platonic_solid", json!({"height":1})),
        ("platonic_solid", json!({"subdivisions_axis":3})),
        ("pyramid", json!({"number_of_sides":2})),
        ("pyramid", json!({"number_of_sides":129})),
        ("pyramid", json!({"number_of_sides":4.0})),
        ("pyramid", json!({"number_of_sides":null})),
        ("pyramid", json!({"number_of_sides":"4"})),
        ("pyramid", json!({"side_length":0})),
        ("pyramid", json!({"side_length":1000001})),
        ("pyramid", json!({"side_length":null})),
        ("pyramid", json!({"length":1})),
        ("pyramid", json!({"radius":1})),
        ("prism", json!({"number_of_sides":2})),
        ("prism", json!({"number_of_sides":129})),
        ("prism", json!({"length":0})),
        ("prism", json!({"length":-1})),
        ("prism", json!({"side_length":-1})),
        ("prism", json!({"axis":"w"})),
        ("prism", json!({"subdivisions_axis":3})),
        ("cube", json!({"number_of_sides":4})),
        ("cube", json!({"solid":"cube"})),
        ("torus", json!({"solid":"octahedron"})),
    ];
    for (primitive, options) in cases {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":options}),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {primitive} {options}");
        };
        assert_eq!(
            error.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{primitive} {options}"
        );
        assert_eq!(editor.history.scene(), &scene, "{primitive} {options}");
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(
            (editor.selected(), editor.selected_path().to_vec()),
            selected
        );
        assert!(!editor.history.can_redo());
    }
    for arguments in [
        json!({"scene_token":token,"primitive":"disc"}),
        json!({"scene_token":token,"primitive":"cube","extra":1}),
        json!({"scene_token":token,"options":{}}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            arguments.clone(),
        )
        .await;
        assert!(
            matches!(
                rejected,
                Err(rmcp::ServiceError::McpError(ref error))
                    if error.code == rmcp::model::ErrorCode::INVALID_PARAMS
            ),
            "{arguments}"
        );
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// The cap subdivision maximum is 128 in the schema and in behaviour. Axis 20 keeps both builds
/// inside the 20,000-face budget: the cylinder has 20 + 2 * 20 * 128 = 5,140 faces, the cone
/// 20 + 20 * 128 = 2,580.
#[tokio::test]
async fn cap_subdivisions_accept_128_and_reject_129_matching_the_schema() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let catalog = client.peer().list_tools(None).await.unwrap();
    let schema = serde_json::to_value(
        catalog
            .tools
            .iter()
            .find(|t| t.name == "create_primitive")
            .unwrap(),
    )
    .unwrap();
    for (primitive, definition, faces) in [
        ("cylinder", "CylinderOptions", 5_140),
        ("cone", "ConeOptions", 2_580),
    ] {
        let caps = &schema["inputSchema"]["$defs"][definition]["properties"]["subdivisions_caps"];
        assert_eq!(
            (&caps["minimum"], &caps["maximum"]),
            (&json!(0), &json!(128)),
            "{primitive}"
        );
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":{"subdivisions_axis":20,"subdivisions_caps":128}}),
        )
        .await;
        assert_eq!(created.is_error, Some(false), "{primitive} 128");
        let counted = counts(&inspect(&mut editor, &context, client.peer()).await);
        assert_eq!(counted.last().unwrap()[2], faces, "{primitive} faces");
        let scene = editor.history.scene().clone();
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":{"subdivisions_axis":20,"subdivisions_caps":129}}),
        )
        .await;
        assert!(
            matches!(
                rejected,
                Err(rmcp::ServiceError::McpError(ref e)) if e.code == rmcp::model::ErrorCode::INVALID_PARAMS
            ),
            "{primitive} 129"
        );
        assert_eq!(
            editor.history.scene(),
            &scene,
            "{primitive} 129 leaves the scene"
        );
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn round_cap_reaches_the_mesh_only_with_cap_subdivisions() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    // Radius 1, height 2 on Y: flat caps stop at |y| = 1, a round cap domes up to 1 beyond.
    let cases = [
        (
            "cylinder",
            json!({"subdivisions_caps":2,"round_cap":true}),
            2.0,
        ),
        (
            "cylinder",
            json!({"subdivisions_caps":2,"round_cap":false}),
            1.0,
        ),
        (
            "cylinder",
            json!({"subdivisions_caps":0,"round_cap":true}),
            1.0,
        ),
        ("cone", json!({"subdivisions_caps":2,"round_cap":true}), 2.0),
        ("cone", json!({"subdivisions_caps":0,"round_cap":true}), 1.0),
    ];
    for (primitive, options, extent) in cases {
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":options}),
        )
        .await
        .structured_content
        .unwrap();
        let inspected = inspect(&mut editor, &context, client.peer()).await;
        let nodes = inspected["inspection"]["nodes"].as_array().unwrap();
        let shape = nodes
            .iter()
            .find(|n| n["id"] == created["node_id"])
            .unwrap()["shape_ids"][0]
            .clone();
        let vertices = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_vertices",
            json!({"shape_id":shape}),
        )
        .await
        .structured_content
        .unwrap();
        let lowest = vertices["inspection"]["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p[1].as_f64().unwrap())
            .fold(f64::MAX, f64::min);
        // The cone's top is its apex at +1, so its extent shows on the base side.
        assert!(
            (lowest + extent).abs() < 1e-5,
            "{primitive} {options}: lowest y {lowest}"
        );
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn optioned_create_obeys_the_scene_token_draft_modal_and_close_guards() {
    guards("cube", json!({"width":2,"subdivisions_width":3})).await;
    for primitive in ["cylinder", "cone"] {
        guards(primitive, json!({"subdivisions_caps":2,"round_cap":true})).await;
    }
}

async fn guards(primitive: &str, options: Value) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":Uuid::new_v4().to_string(),"primitive":primitive,"options":options}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.history.scene(), &before);

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":primitive,"options":options}),
    )
    .await;
    assert_eq!(
        draft.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.name, "human_draft");
    assert_eq!(editor.history.scene(), &before);
    editor.sync_fields();

    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(pending["outcome"], "pending_confirmation");
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":primitive,"options":options}),
    )
    .await;
    assert_eq!(
        modal.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();

    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    editor.request_close();
    let closing = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":primitive,"options":options}),
    )
    .await;
    assert_eq!(
        closing.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn optioned_create_on_an_opened_maya_binary_is_refused_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.mb");
    std::fs::write(&path, binary_fixture::specimen()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let original = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":original,"path":path.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    let token = opened["scene_token"].as_str().unwrap().to_owned();
    let baseline = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"sphere","options":{"radius":2}}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &baseline);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
