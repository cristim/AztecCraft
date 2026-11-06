use super::*;
use crate::app::tests::frame_with;
use crate::app::{channel_edit_id, text_edit_focused};
use crate::editor_operations::SelectionMode;
use crate::editor_operations::SidebarPanel::{AttributeEditor as AE, ChannelBox as CB};
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2, accesskit};

#[allow(dead_code)]
mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}
#[allow(dead_code)]
mod binary_fixture_appearance {
    use super::binary_fixture as wire;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_appearance.rs"
    ));
}
mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

const SIZE: Vec2 = egui::vec2(960.0, 640.0);

fn setup() -> (Editor, egui::Context) {
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    editor.create(Primitive::Cube);
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    (editor, context)
}

fn key_event(key: Key, modifiers: Modifiers) -> Vec<Event> {
    vec![
        Event::ModifiersChanged(modifiers),
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        },
        Event::ModifiersChanged(Modifiers::NONE),
    ]
}

/// Presses then releases `A` like a keyboard; returns the press frame's output.
fn press_a(editor: &mut Editor, context: &egui::Context, modifiers: Modifiers) -> egui::FullOutput {
    let output = frame_with(editor, context, SIZE, 1.0, key_event(Key::A, modifiers));
    frame_with(
        editor,
        context,
        SIZE,
        1.0,
        vec![Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    output
}

fn ctrl_a(editor: &mut Editor, context: &egui::Context) -> egui::FullOutput {
    press_a(editor, context, Modifiers::CTRL)
}

fn click_at(editor: &mut Editor, context: &egui::Context, at: Pos2) {
    frame_with(editor, context, SIZE, 1.0, vec![Event::PointerMoved(at)]);
    for pressed in [true, false] {
        frame_with(
            editor,
            context,
            SIZE,
            1.0,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
}

fn inspected(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(editor.inspect(false).unwrap()).unwrap()
}

fn text_drawn(output: &egui::FullOutput, text: &str) -> bool {
    output.shapes.iter().any(
        |shape| matches!(&shape.shape, egui::Shape::Text(drawn) if drawn.galley.job.text == text),
    )
}

/// Sidebar panels drawn this frame, by their header text.
fn panels_drawn(output: &egui::FullOutput) -> (bool, bool) {
    (
        text_drawn(output, "Channel Box"),
        text_drawn(output, "Attribute Editor"),
    )
}

fn label_center(output: &egui::FullOutput, label: &str) -> Pos2 {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(label))
        .unwrap_or_else(|| panic!("missing control {label}"))
        .1
        .bounds()
        .map(|bounds| {
            Pos2::new(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            )
        })
        .unwrap()
}

#[test]
fn ctrl_a_target_follows_the_transition_table() {
    assert_eq!(ctrl_a_target(Some(CB)), AE);
    assert_eq!(ctrl_a_target(Some(AE)), CB);
    assert_eq!(ctrl_a_target(None), AE);
    for (current, target) in [(Some(CB), AE), (Some(AE), CB), (None, AE)] {
        let mut editor = Editor {
            sidebar_dock: current,
            ..Editor::default()
        };
        editor
            .apply_operation(Operation::SetSidebarPanelVisible {
                panel: ctrl_a_target(current),
                visible: true,
            })
            .unwrap();
        assert_eq!(editor.sidebar_dock, Some(target));
    }
}

#[test]
fn ctrl_a_cycles_the_dock_and_exactly_one_panel_is_drawn_per_frame() {
    let (mut editor, context) = setup();
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(panels_drawn(&output), (true, false));
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(AE));
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(panels_drawn(&output), (false, true));
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(CB));
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(panels_drawn(&output), (true, false));
    editor
        .apply_operation(Operation::SetSidebarPanelVisible {
            panel: CB,
            visible: false,
        })
        .unwrap();
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(panels_drawn(&output), (false, false));
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(AE));
}

#[test]
fn ctrl_a_ignores_other_modifiers_text_focus_and_modal() {
    let (mut editor, context) = setup();
    for modifiers in [
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::MAC_CMD,
        Modifiers::CTRL | Modifiers::SHIFT,
    ] {
        press_a(&mut editor, &context, modifiers);
        assert_eq!(editor.sidebar_dock, Some(CB), "{modifiers:?}");
    }
    context.memory_mut(|memory| memory.request_focus(channel_edit_id(0)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(text_edit_focused(&context));
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(CB), "text focus keeps select-all");
    context.memory_mut(|memory| memory.surrender_focus(channel_edit_id(0)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(!text_edit_focused(&context));
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(CB), "modal blocks the shortcut");
}

#[test]
fn ctrl_a_with_a_pending_draft_commits_a_valid_one_and_refuses_an_invalid_one() {
    let (mut editor, context) = setup();
    let scene = editor.history.scene().clone();
    editor.channels[0] = "invalid".into();
    editor.channel_intents[0] = true;
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(CB));
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.channels[0], "invalid");
    editor.channels[0] = "4".into();
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(AE));
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        4.0
    );
}

#[test]
fn attribute_editor_edits_share_the_channel_drafts_and_history() {
    let (mut editor, context) = setup();
    ctrl_a(&mut editor, &context);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(panels_drawn(&output), (false, true));
    let scene = editor.history.scene().clone();
    // An untouched field never writes back.
    context.memory_mut(|memory| memory.request_focus(channel_edit_id(1)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        key_event(Key::Enter, Modifiers::NONE),
    );
    assert_eq!(editor.history.scene(), &scene);
    // A typed edit commits through the same draft and widget id as the Channel Box.
    context.memory_mut(|memory| memory.request_focus(channel_edit_id(0)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        vec![Event::Text("5".into())],
    );
    assert!(editor.has_channel_draft());
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        key_event(Key::Enter, Modifiers::NONE),
    );
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        5.0
    );
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.sidebar_dock, Some(AE));
    editor.history_step(true);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        5.0
    );
}

#[test]
fn clicking_a_shape_tab_selects_it_and_the_tab_survives_undo() {
    let (mut editor, context) = setup();
    ctrl_a(&mut editor, &context);
    let shape = editor.history.scene().shapes()[0].id();
    let name = editor.history.scene().shapes()[0].name().to_owned();
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let at = label_center(&output, &name);
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        vec![Event::PointerMoved(at)],
    );
    for pressed in [true, false] {
        frame_with(
            &mut editor,
            &context,
            SIZE,
            1.0,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    assert_eq!(
        editor.attribute_editor_tab,
        Some(AttributeEditorTab::Shape(shape))
    );
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(text_drawn(&output, &format!("Shape: {name}")));
    let pin = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Pin Tab"))
        .expect("Pin Tab is drawn")
        .1
        .is_disabled();
    assert!(pin);
    editor
        .apply_operation(Operation::Rename {
            id: editor.selected().unwrap(),
            name: "renamed".into(),
        })
        .unwrap();
    editor.history_step(false);
    assert_eq!(
        editor.attribute_editor_tab,
        Some(AttributeEditorTab::Shape(shape))
    );
}

#[test]
fn the_status_line_toggle_drives_the_attribute_editor_dock() {
    let (mut editor, context) = setup();
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let at = label_center(&output, "Attribute Editor");
    for expected in [Some(AE), None] {
        frame_with(
            &mut editor,
            &context,
            SIZE,
            1.0,
            vec![Event::PointerMoved(at)],
        );
        for pressed in [true, false] {
            frame_with(
                &mut editor,
                &context,
                SIZE,
                1.0,
                vec![Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        assert_eq!(editor.sidebar_dock, expected);
    }
    let _ = accesskit::Role::Button;
}

/// Opens `bytes` as `name` in a fresh editor, selects the first node and makes the Attribute
/// Editor the active panel.
fn open_with_attribute_editor(name: &str, bytes: &[u8]) -> (Editor, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let id = editor.history.scene().nodes()[0].id();
    editor.select(Some(id));
    editor
        .apply_operation(Operation::SetSidebarPanelVisible {
            panel: AE,
            visible: true,
        })
        .unwrap();
    (editor, directory)
}

/// Texts drawn by the Attribute Editor body for one frame with the given events.
fn attribute_editor_texts(
    editor: &mut Editor,
    context: &egui::Context,
    events: Vec<Event>,
) -> Vec<String> {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(420.0, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| editor.attribute_editor(ui),
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn read_only_ascii_attribute_editor_is_disabled_with_the_shared_reason() {
    let (mut editor, _directory) =
        open_with_attribute_editor("curve.ma", &ascii_fixture::specimen());
    let before = editor.history.scene().clone();
    let fields = (editor.name.clone(), editor.channels.clone());
    let reason = editor
        .history
        .scene()
        .authoring_read_only_reason()
        .unwrap()
        .to_string();
    for text_id in [crate::app::name_edit_id(), channel_edit_id(0)] {
        let context = egui::Context::default();
        context.memory_mut(|memory| memory.request_focus(text_id));
        let texts = attribute_editor_texts(
            &mut editor,
            &context,
            vec![
                Event::Text("must not become a draft".into()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(texts.iter().any(|text| text.contains(&reason)), "{texts:?}");
        assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
        assert!(!context.memory(|memory| memory.has_focus(text_id)));
        assert!(!editor.has_draft());
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn curve_only_scene_shows_the_transform_tab_and_curve_information() {
    let (mut editor, _directory) =
        open_with_attribute_editor("curve.ma", &ascii_fixture::specimen());
    assert!(editor.history.scene().shapes().is_empty());
    let node = editor.selected().unwrap();
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(node))
    );
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    let name = editor.history.scene().node(node).unwrap().name().to_owned();
    assert!(texts.contains(&name), "transform tab {texts:?}");
    assert!(
        texts.iter().any(|text| text.starts_with("Curves:")),
        "curve note {texts:?}"
    );
    assert!(texts.iter().any(|text| text == "Translate X"), "{texts:?}");
    assert!(!texts.iter().any(|text| text.ends_with(" material")));
}

#[test]
fn radian_scene_displays_and_commits_rotation_in_scene_units() {
    let source = binary_fixture::scene_with_units(
        &[
            binary_fixture::group(
                b"XFRM",
                &[
                    binary_fixture::creation(b"object", None),
                    binary_fixture::triple(b"t", [1.0, 2.0, 3.0]),
                    binary_fixture::triple(b"r", [0.5, 0.0, 0.0]),
                ]
                .concat(),
                4,
            ),
            binary_fixture::group(
                b"DMSH",
                &[
                    binary_fixture::creation(b"shape", Some(b"object")),
                    binary_fixture::chunk(
                        b"MESH",
                        &binary_fixture::mesh_payload(&[0, 1, 0x6000_0002]),
                        8,
                    ),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        b"cm",
        b"rad",
    );
    let (mut editor, _directory) = open_with_attribute_editor("radian.mb", &source);
    assert_eq!(editor.channels[3], "0.5", "displayed in radians");
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    assert!(
        texts.iter().any(|text| text == "Rotation in radians | XYZ"),
        "{texts:?}"
    );
    editor.channels[3] = "1".into();
    editor.channel_intents[3] = true;
    assert!(editor.commit_pending());
    let id = editor.history.scene().nodes()[0].id();
    let rotation = editor
        .history
        .scene()
        .internal_transform(id)
        .unwrap()
        .rotation_radians[0];
    assert!((rotation - 1.0).abs() < 1e-12, "{rotation}");
    assert_eq!(editor.channels[3], "1");
}

#[test]
fn an_open_popup_blocks_ctrl_a() {
    let (mut editor, context) = setup();
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    click_at(&mut editor, &context, label_center(&output, "Menu set"));
    assert!(egui::Popup::is_any_open(&context));
    ctrl_a(&mut editor, &context);
    assert_eq!(editor.sidebar_dock, Some(CB), "popup swallows the shortcut");
}

/// One transform with `count` direct mesh shapes named `shape0`, `shape1`, ...
fn many_shapes(count: usize) -> Vec<u8> {
    let mut children = binary_fixture::group(
        b"XFRM",
        &[
            binary_fixture::creation(b"object", None),
            binary_fixture::triple(b"t", [0.0, 0.0, 0.0]),
        ]
        .concat(),
        4,
    );
    for index in 0..count {
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
    binary_fixture::scene_with_units(&children, b"cm", b"deg").to_vec()
}

fn shape_ids(editor: &Editor) -> Vec<NativeId> {
    let transform = editor.history.scene().nodes()[0].transform_id();
    editor.direct_shapes(transform).collect()
}

#[test]
fn two_direct_shapes_have_independent_tabs_that_drive_the_material_shape() {
    let (mut editor, _directory) = open_with_attribute_editor("two.mb", &many_shapes(2));
    let shapes = shape_ids(&editor);
    assert_eq!(shapes.len(), 2);
    assert_eq!(editor.material_shape, None, "two shapes need a choice");
    for shape in [shapes[1], shapes[0], shapes[1]] {
        editor
            .apply_operation(Operation::SelectAttributeEditorTab {
                tab: AttributeEditorTab::Shape(shape),
            })
            .unwrap();
        assert_eq!(editor.material_shape, Some(shape));
        assert_eq!(
            editor.effective_attribute_editor_tab(),
            Some(AttributeEditorTab::Shape(shape))
        );
    }
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    for name in ["shape0", "shape1"] {
        assert!(texts.iter().any(|text| text == name), "{texts:?}");
    }
    assert!(
        texts.iter().any(|text| text == "Shape: shape1"),
        "{texts:?}"
    );
}

#[test]
fn shape_tabs_beyond_the_cap_are_refused_naming_the_cap() {
    let (mut editor, _directory) =
        open_with_attribute_editor("many.mb", &many_shapes(MAX_SHAPE_TABS + 3));
    let shapes = shape_ids(&editor);
    assert_eq!(shapes.len(), MAX_SHAPE_TABS + 3);
    editor
        .apply_operation(Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Shape(shapes[MAX_SHAPE_TABS - 1]),
        })
        .unwrap();
    let kept = editor.attribute_editor_tab;
    let before = inspected(&editor);
    for tab in [
        AttributeEditorTab::Shape(shapes[MAX_SHAPE_TABS]),
        AttributeEditorTab::Material(shapes[MAX_SHAPE_TABS + 2]),
    ] {
        for remote_caller in [false, true] {
            let operation = Operation::SelectAttributeEditorTab { tab };
            let result = if remote_caller {
                crate::app::tests::remote(&mut editor, operation)
            } else {
                editor.apply_operation(operation)
            };
            let Err(OperationError::InvalidInput(message)) = result else {
                panic!("tab beyond the cap must be refused");
            };
            assert!(message.contains(&MAX_SHAPE_TABS.to_string()), "{message}");
            assert_eq!(editor.attribute_editor_tab, kept);
            assert_eq!(inspected(&editor), before);
        }
    }
    // A stored tab that ends up beyond the cap never resolves, so the active tab is always drawn.
    editor.attribute_editor_tab = Some(AttributeEditorTab::Shape(shapes[MAX_SHAPE_TABS]));
    assert!(matches!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(_))
    ));
    editor.attribute_editor_tab = kept;
    // Exactly the capped tabs are drawn, and the omission is stated.
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    let drawn = texts
        .iter()
        .filter(|text| text.starts_with("shape") && text[5..].parse::<usize>().is_ok())
        .count();
    assert_eq!(drawn, MAX_SHAPE_TABS, "{texts:?}");
    assert!(
        texts.iter().any(|text| text.contains("3 more shapes")),
        "{texts:?}"
    );
}

#[test]
fn the_material_tab_edits_only_its_own_shape() {
    let (mut editor, _directory) = open_with_attribute_editor(
        "material.mb",
        &binary_fixture_appearance::appearance_specimen_with_users(2),
    );
    let shape = editor.history.scene().shapes()[0].id();
    editor
        .apply_operation(Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape),
        })
        .unwrap();
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    assert!(texts.iter().any(|text| text == "Diffuse"), "{texts:?}");
    // Moved away while hidden (the Channel Box combo): showing the panel re-binds it.
    editor
        .apply_operation(Operation::SetSidebarPanelVisible {
            panel: CB,
            visible: true,
        })
        .unwrap();
    editor.material_shape = None;
    editor
        .apply_operation(Operation::SetSidebarPanelVisible {
            panel: AE,
            visible: true,
        })
        .unwrap();
    assert_eq!(editor.material_shape, Some(shape));
    assert_eq!(
        editor.material_draft.as_ref().map(|draft| draft.shape),
        Some(shape)
    );
    // If the selection ever diverges, the editor is withheld instead of editing another shape.
    editor.material_shape = None;
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    assert!(!texts.iter().any(|text| text == "Diffuse"), "{texts:?}");
    assert!(
        texts.iter().any(|text| text.contains("out of sync")),
        "{texts:?}"
    );
}

#[test]
fn face_mode_keeps_the_selected_tab() {
    let (mut editor, _directory) = open_with_attribute_editor("two.mb", &many_shapes(2));
    let shapes = shape_ids(&editor);
    editor
        .apply_operation(Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Shape(shapes[1]),
        })
        .unwrap();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    assert_eq!(
        editor.attribute_editor_tab,
        Some(AttributeEditorTab::Shape(shapes[1]))
    );
    let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
    let instance = view
        .instances()
        .iter()
        .find(|instance| instance.shape() == shapes[1])
        .unwrap();
    let target = azteccraft::native_scene::FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 0,
    };
    editor
        .apply_operation(Operation::SelectFace {
            target: Some(target),
        })
        .unwrap();
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Shape(shapes[1])),
        "tab preserved with a face selected"
    );
    let texts = attribute_editor_texts(&mut editor, &egui::Context::default(), vec![]);
    assert!(
        texts.iter().any(|text| text == "Shape: shape1"),
        "{texts:?}"
    );
}

#[test]
fn a_draft_typed_in_either_panel_is_committed_by_the_dock_switch_and_focus_is_released() {
    for start_in_attribute_editor in [false, true] {
        let (mut editor, context) = setup();
        if start_in_attribute_editor {
            ctrl_a(&mut editor, &context);
        }
        let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
        let toggle = label_center(
            &output,
            if start_in_attribute_editor {
                "Channel Box / Layer Editor"
            } else {
                "Attribute Editor"
            },
        );
        context.memory_mut(|memory| memory.request_focus(channel_edit_id(0)));
        frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
        frame_with(
            &mut editor,
            &context,
            SIZE,
            1.0,
            vec![Event::Text("6".into())],
        );
        assert!(editor.has_channel_draft() && text_edit_focused(&context));
        // Ctrl+A is the select-all of the focused field, not a dock switch.
        let dock = editor.sidebar_dock;
        ctrl_a(&mut editor, &context);
        assert_eq!(editor.sidebar_dock, dock);
        click_at(&mut editor, &context, toggle);
        assert_ne!(editor.sidebar_dock, dock, "toggle switched the dock");
        assert!(!editor.has_draft(), "valid draft committed");
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            6.0
        );
        frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
        assert!(!text_edit_focused(&context), "focus is released");
        ctrl_a(&mut editor, &context);
        assert_eq!(
            editor.sidebar_dock, dock,
            "Ctrl+A works again after the switch"
        );
    }
}

#[test]
fn transform_attributes_render_read_only_for_a_node_with_extras() {
    let body = "//Maya ASCII 2016 scene\nrequires maya \"2016\";\n\
        currentUnit -l centimeter -a degree -t film;\n\
        createNode transform -n \"a\"; setAttr \".ro\" 1; setAttr \".sh\" -type \"double3\" 0.5 0 0;\n";
    let (mut editor, _directory) = open_with_attribute_editor("extras.ma", body.as_bytes());
    let context = egui::Context::default();
    context.enable_accesskit();
    let before = editor.history.scene().clone();
    let texts = attribute_editor_texts(&mut editor, &context, vec![]);
    // Maya 2026 order: Shear, Rotate Order, Rotate Axis, then a separate Pivots heading.
    let position = |wanted: &str| {
        texts
            .iter()
            .position(|text| text == wanted)
            .unwrap_or_else(|| panic!("{wanted}: {texts:?}"))
    };
    let order = [
        "Transform Attributes (read-only)",
        "Shear",
        "0.5 0 0",
        "Rotate Order",
        "yzx",
        "Rotate Axis",
        "Pivots (read-only)",
        "Rotate Pivot Translate",
        "Scale Pivot Translate",
    ]
    .map(position);
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    // Rows are labels: the draft fields and the scene stay as they were.
    assert_eq!(editor.history.scene(), &before);
    // A default node shows the same rows with default values.
    let (mut plain, _plain_directory) = open_with_attribute_editor(
        "plain.ma",
        b"//Maya ASCII 2016 scene\nrequires maya \"2016\";\ncurrentUnit -l centimeter -a degree -t film;\ncreateNode transform -n \"a\";\n",
    );
    let texts = attribute_editor_texts(&mut plain, &context, vec![]);
    assert!(texts.iter().any(|text| text == "xyz"), "{texts:?}");
}

#[test]
fn attribute_editor_scrolls_to_the_pivots_at_the_minimum_window_size() {
    use crate::app::tests::{visible_text_rects, wheel_frames};
    let (mut editor, context) = setup();
    ctrl_a(&mut editor, &context);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(editor.sidebar_dock, Some(AE));
    for text in ["Rotate Pivot Translate", "Scale Pivot Translate"] {
        assert!(
            visible_text_rects(&output, text).is_empty(),
            "{text} visible"
        );
    }
    let camera = editor.camera.clone();
    let over_body = Pos2::new(SIZE.x - 100.0, 300.0);
    let output = wheel_frames(&mut editor, &context, SIZE, over_body, 10);
    assert_eq!(editor.camera, camera, "wheel reached the viewport");
    for text in [
        "Pivots (read-only)",
        "Rotate Pivot Translate",
        "Scale Pivot Translate",
    ] {
        assert!(
            !visible_text_rects(&output, text).is_empty(),
            "{text} unreachable"
        );
    }
    // The tab bar stays pinned while the body scrolls, and its tabs still take clicks.
    let tab = editor.history.scene().nodes()[0].name().to_owned();
    assert!(
        !visible_text_rects(&output, &tab).is_empty(),
        "tab bar scrolled away"
    );
    let shape = editor.history.scene().shapes()[0].id();
    let name = editor.history.scene().shapes()[0].name().to_owned();
    click_at(&mut editor, &context, label_center(&output, &name));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Shape(shape))
    );
}

#[test]
fn empty_attribute_editor_shows_no_diagnostics_header() {
    let (mut editor, _directory) =
        open_with_attribute_editor("curve.ma", &ascii_fixture::specimen());
    let reason = editor
        .history
        .scene()
        .authoring_read_only_reason()
        .unwrap()
        .to_string();
    let context = egui::Context::default();
    let selected = attribute_editor_texts(&mut editor, &context, vec![]);
    assert!(selected.contains(&reason), "{selected:?}");
    editor.select(None);
    let texts = attribute_editor_texts(&mut editor, &context, vec![]);
    assert_eq!(
        texts,
        [
            "Attribute Editor",
            "Select an object to edit its attributes."
        ]
    );
}
