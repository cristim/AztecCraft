use super::*;
use crate::app::tests::remote;
use azteccraft::native_scene::{AngularUnit, LinearUnit, Units};
use azteccraft::preferences::{Baseline, PreferencesError};
use std::path::PathBuf;

#[allow(dead_code)]
mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

fn v(year: u64) -> MayaVersion {
    MayaVersion::try_from(year).unwrap()
}

fn set(version: u64) -> Operation {
    Operation::SetCompatibilityVersion {
        version: v(version),
    }
}

fn writable(dir: &tempfile::TempDir) -> (Editor, PathBuf) {
    let path = dir.path().join("preferences.json");
    let loaded = azteccraft::preferences::load(&path);
    let editor = Editor {
        preferences: loaded.preferences,
        preferences_store: loaded.store,
        ..Editor::default()
    };
    (editor, path)
}

fn blocked(dir: &tempfile::TempDir) -> (Editor, PathBuf) {
    let path = dir.path().join("preferences.json");
    std::fs::write(&path, b"not json").unwrap();
    let loaded = azteccraft::preferences::load(&path);
    assert!(matches!(loaded.store, Store::Blocked { .. }));
    let editor = Editor {
        preferences: loaded.preferences,
        preferences_store: loaded.store,
        ..Editor::default()
    };
    (editor, path)
}

#[test]
fn a_default_editor_never_touches_a_file() {
    let editor = Editor::default();
    assert_eq!(editor.preferences_store, Store::SessionOnly);
    assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
}

#[test]
fn constructing_through_new_stays_session_only_and_resolves_no_platform_path() {
    PLATFORM_PATH_RESOLUTIONS.with(|count| count.set(0));
    let editor = Editor::with_renderer(None);
    assert_eq!(editor.preferences_store, Store::SessionOnly);
    assert_eq!(PLATFORM_PATH_RESOLUTIONS.with(|count| count.get()), 0);
}

#[test]
fn editor_new_is_a_thin_wrapper_that_never_loads_preferences() {
    let source = include_str!("../../mod.rs");
    let start = source.find("pub fn new(").expect("Editor::new exists");
    let body = &source[start..];
    let body = &body[..body.find("\n    }\n").expect("Editor::new ends")];
    assert!(body.contains("Self::with_renderer(Some(renderer))"));
    assert!(!body.contains("load_preferences"), "{body}");
}

#[test]
fn set_changed_persists_and_applies() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = writable(&dir);
    assert!(matches!(
        editor.apply_operation(set(2015)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2015));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("2015"), "{text}");
}

#[test]
fn set_unchanged_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = writable(&dir);
    assert!(matches!(
        editor.apply_operation(set(2027)),
        Ok(OperationReply::Unchanged)
    ));
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn a_write_failure_keeps_the_value_and_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = writable(&dir);
    editor.apply_operation(set(2020)).unwrap();
    let before = std::fs::read(&path).unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = editor.apply_operation(set(2012));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        result,
        Err(OperationError::Preferences(PreferencesError::Write(_)))
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2020));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(matches!(
        editor.preferences_store,
        Store::Writable {
            baseline: Baseline::Bytes(_),
            ..
        }
    ));
}

#[test]
fn a_blocked_store_applies_the_value_for_the_session_only() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = blocked(&dir);
    assert!(matches!(
        editor.apply_operation(set(2018)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2018));
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
    assert!(matches!(editor.preferences_store, Store::Blocked { .. }));
}

#[test]
fn a_session_only_store_applies_the_value() {
    let mut editor = Editor::default();
    assert!(matches!(
        editor.apply_operation(set(2016)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2016));
}

#[test]
fn reset_works_only_when_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, _) = writable(&dir);
    editor.apply_operation(set(2015)).unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::ResetPreferences),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2015));

    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = blocked(&dir);
    editor.apply_operation(set(2015)).unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::ResetPreferences),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
    assert!(!path.exists());
    assert!(dir.path().join("preferences.json.bad-1").exists());
    assert!(matches!(editor.preferences_store, Store::Writable { .. }));
}

#[test]
fn preferences_operations_leave_the_scene_alone() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let (undo, redo, token) = (
        editor.history.can_undo(),
        editor.history.can_redo(),
        editor.scene_token,
    );
    let undo_label = editor.history.undo_label();
    let dirty = editor.is_dirty();
    editor.apply_operation(set(2015)).unwrap();
    editor.apply_operation(Operation::ResetPreferences).unwrap();
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), undo_label);
    assert_eq!(
        (editor.history.can_undo(), editor.history.can_redo()),
        (undo, redo)
    );
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
}

#[test]
fn a_pending_value_does_not_dirty_the_scene_or_count_as_a_draft() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    assert!(!editor.is_dirty());
    assert!(!editor.has_draft());
}

#[test]
fn remote_set_and_reset_are_busy_while_a_value_is_pending() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    for operation in [set(2020), Operation::ResetPreferences] {
        assert!(matches!(
            remote(&mut editor, operation),
            Err(OperationError::BusyEditing)
        ));
    }
    assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
    editor.preferences_window.pending = None;
    assert!(matches!(
        remote(&mut editor, set(2020)),
        Ok(OperationReply::Changed)
    ));
}

#[test]
fn a_pending_value_does_not_block_other_remote_work() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    remote(
        &mut editor,
        Operation::Orbit {
            radians: [0.1, 0.0],
        },
    )
    .unwrap();
    assert!(matches!(
        remote(&mut editor, Operation::SetGridVisible { visible: false }),
        Ok(OperationReply::Changed)
    ));
}

#[test]
fn native_set_is_not_blocked_by_a_pending_value() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    assert!(matches!(
        editor.apply_operation(set(2020)),
        Ok(OperationReply::Changed)
    ));
}

mod window {
    use super::*;
    use crate::app::tests::frame_with;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, accesskit};

    const SIZE: Vec2 = egui::vec2(1200.0, 800.0);

    struct Control {
        role: accesskit::Role,
        label: String,
        rect: Rect,
        disabled: bool,
    }

    fn controls(output: &egui::FullOutput) -> Vec<Control> {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accesskit enabled");
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                Some(Control {
                    role: node.role(),
                    label: node.label()?.to_owned(),
                    rect: Rect::from_min_max(
                        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                    disabled: node.is_disabled(),
                })
            })
            .collect()
    }

    fn button(output: &egui::FullOutput, label: &str) -> Option<Control> {
        controls(output)
            .into_iter()
            .filter(|c| c.role == accesskit::Role::Button && c.label == label)
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
    }

    fn setup(editor: Editor) -> (Editor, egui::Context) {
        let context = egui::Context::default();
        context.enable_accesskit();
        let editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..editor
        };
        (editor, context)
    }

    fn run(editor: &mut Editor, ctx: &egui::Context, events: Vec<Event>) -> egui::FullOutput {
        frame_with(editor, ctx, SIZE, 1.0, events)
    }

    fn settle(editor: &mut Editor, ctx: &egui::Context) -> egui::FullOutput {
        let mut last = run(editor, ctx, vec![]);
        // The window sizes itself over several frames.
        for _ in 0..8 {
            last = run(editor, ctx, vec![]);
        }
        last
    }

    fn click(editor: &mut Editor, ctx: &egui::Context, at: Pos2) {
        run(editor, ctx, vec![Event::PointerMoved(at)]);
        for pressed in [true, false] {
            run(
                editor,
                ctx,
                vec![Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }

    fn press(editor: &mut Editor, ctx: &egui::Context, label: &str) {
        let output = settle(editor, ctx);
        let found = button(&output, label).unwrap_or_else(|| panic!("button {label}"));
        assert!(!found.disabled, "{label} must be enabled");
        click(editor, ctx, found.rect.center());
    }

    fn open_window(editor: &mut Editor, ctx: &egui::Context) {
        editor.open_preferences();
        settle(editor, ctx);
    }

    fn window_shown(output: &egui::FullOutput) -> bool {
        button(output, "Save").is_some()
    }

    fn with_store(dir: &tempfile::TempDir) -> (Editor, egui::Context, PathBuf) {
        let (editor, path) = writable(dir);
        let (editor, ctx) = setup(editor);
        (editor, ctx, path)
    }

    fn untouched(editor: &Editor, path: &std::path::Path) {
        assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
        assert!(!path.exists());
        assert_eq!(editor.preferences_window.pending, None);
    }

    #[test]
    fn categories_are_in_the_maya_order() {
        assert_eq!(PREFERENCE_CATEGORIES.len(), 39);
        assert_eq!(
            PREFERENCE_CATEGORIES,
            [
                "Animation",
                "Applications",
                "Assets",
                "Blue Pencil",
                "Cached Playback",
                "Cameras",
                "Color Management",
                "Display",
                "DX11 Shader",
                "Dynamics",
                "Files/Projects",
                "File References",
                "Font",
                "GLSL Shader",
                "GPU Cache",
                "Help",
                "Hypershade",
                "Interface",
                "Kinematics",
                "Manipulators",
                "Modeling",
                "Modules",
                "Node Editor",
                "NURBS",
                "Polygons",
                "Rendering",
                "Save Actions",
                "Search",
                "Security",
                "Selection",
                "Settings",
                "Snapping",
                "Sound",
                "Subdivs",
                "Time Slider",
                "UI Elements",
                "ViewCube",
                "Undo",
                "XGen",
            ]
        );
    }

    #[test]
    fn the_windows_menu_path_opens_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        let output = settle(&mut editor, &ctx);
        assert!(!window_shown(&output));
        let windows = controls(&output)
            .into_iter()
            .find(|c| c.label == "Windows" && c.rect.top() < 30.0)
            .expect("Windows menu")
            .rect
            .center();
        click(&mut editor, &ctx, windows);
        let opened = settle(&mut editor, &ctx);
        let submenu = controls(&opened)
            .into_iter()
            .find(|c| c.label.starts_with("Settings/Preferences"))
            .expect("Settings/Preferences submenu")
            .rect
            .center();
        run(&mut editor, &ctx, vec![Event::PointerMoved(submenu)]);
        let hovered = settle(&mut editor, &ctx);
        let row = controls(&hovered)
            .into_iter()
            .find(|c| c.role == accesskit::Role::Button && c.label == "Preferences")
            .expect("Preferences row");
        assert!(!row.disabled);
        click(&mut editor, &ctx, row.rect.center());
        assert!(editor.preferences_window.open);
        assert!(window_shown(&settle(&mut editor, &ctx)));
    }

    #[test]
    fn only_the_settings_category_is_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        let output = settle(&mut editor, &ctx);
        let shown = controls(&output);
        for name in PREFERENCE_CATEGORIES {
            let row = shown
                .iter()
                .find(|c| c.label == name && c.rect.left() > 0.0 && c.rect.top() > 40.0)
                .unwrap_or_else(|| panic!("category {name}"));
            assert_eq!(row.disabled, name != "Settings", "{name}");
        }
    }

    #[test]
    fn edit_menu_rows_are_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        let output = settle(&mut editor, &ctx);
        let edit = controls(&output)
            .into_iter()
            .filter(|c| c.label == "Edit")
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .expect("window Edit menu")
            .rect
            .center();
        click(&mut editor, &ctx, edit);
        let opened = settle(&mut editor, &ctx);
        for label in ["Revert to Saved", "Restore Default Settings"] {
            let row = button(&opened, label).unwrap_or_else(|| panic!("row {label}"));
            assert!(row.disabled, "{label}");
        }
    }

    #[test]
    fn cancel_discards_a_pending_value_without_an_operation() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = with_store(&dir);
        open_window(&mut editor, &ctx);
        editor.preferences_window.pending = Some(v(2015));
        press(&mut editor, &ctx, "Cancel");
        assert!(!window_shown(&settle(&mut editor, &ctx)));
        assert!(!editor.preferences_window.open);
        untouched(&editor, &path);
    }

    #[test]
    fn the_x_button_discards_a_pending_value_without_an_operation() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = with_store(&dir);
        open_window(&mut editor, &ctx);
        editor.preferences_window.pending = Some(v(2015));
        press(&mut editor, &ctx, "Close window");
        assert!(!window_shown(&settle(&mut editor, &ctx)));
        assert!(!editor.preferences_window.open);
        untouched(&editor, &path);
    }

    #[test]
    fn app_close_discards_a_pending_value_without_an_operation() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = with_store(&dir);
        open_window(&mut editor, &ctx);
        editor.preferences_window.pending = Some(v(2015));
        editor.apply_operation(Operation::RequestClose).unwrap();
        assert!(editor.files.is_closing());
        untouched(&editor, &path);
    }

    #[test]
    fn save_is_disabled_while_closing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        assert!(!button(&settle(&mut editor, &ctx), "Save").unwrap().disabled);
        editor.apply_operation(Operation::RequestClose).unwrap();
        assert!(editor.files.is_closing());
        assert!(button(&settle(&mut editor, &ctx), "Save").unwrap().disabled);
    }

    #[test]
    fn save_is_disabled_while_a_file_prompt_is_modal() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        editor.create(Primitive::Cube);
        editor.apply_operation(Operation::RequestClose).unwrap();
        assert!(editor.files.is_modal());
        assert!(button(&settle(&mut editor, &ctx), "Save").unwrap().disabled);
    }

    #[test]
    fn save_persists_the_pending_value_and_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = with_store(&dir);
        open_window(&mut editor, &ctx);
        editor.preferences_window.pending = Some(v(2015));
        press(&mut editor, &ctx, "Save");
        assert_eq!(editor.preferences_window.pending, None);
        assert!(!editor.preferences_window.open);
        assert_eq!(editor.preferences.maya_compatibility_version, v(2015));
        assert!(std::fs::read_to_string(&path).unwrap().contains("2015"));
    }

    #[test]
    fn a_failed_save_keeps_the_pending_value_and_shows_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let (editor, path) = blocked(&dir);
        let (mut editor, ctx) = setup(editor);
        editor.preferences_store = Store::Writable {
            path: path.join("missing").join("preferences.json"),
            baseline: Baseline::Missing,
        };
        open_window(&mut editor, &ctx);
        editor.preferences_window.pending = Some(v(2015));
        press(&mut editor, &ctx, "Save");
        assert_eq!(editor.preferences_window.pending, Some(v(2015)));
        assert!(editor.preferences_window.open);
        assert!(editor.preferences_window.error.is_some());
    }

    #[test]
    fn a_successful_save_replaces_a_stale_error_status() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        editor.error = true;
        editor.status = "Old failure".into();
        editor.preferences_window.pending = Some(v(2015));
        press(&mut editor, &ctx, "Save");
        assert!(!editor.preferences_window.open);
        assert_eq!(editor.preferences_window.pending, None);
        assert!(!editor.error);
        assert_eq!(editor.status, "Preferences saved");
    }

    fn save_status(editor: Editor, expected: &str) -> String {
        let (mut editor, ctx) = setup(editor);
        open_window(&mut editor, &ctx);
        editor.preferences_window.pending = Some(v(2015));
        press(&mut editor, &ctx, "Save");
        assert!(!editor.preferences_window.open);
        assert_eq!(editor.preferences.maya_compatibility_version, v(2015));
        assert_eq!(editor.status, expected);
        editor.status
    }

    #[test]
    fn saving_with_a_blocked_store_states_its_own_reason_without_the_path() {
        for (error, reason) in [
            (
                PreferencesError::UnknownSchema,
                "the preferences file has an unknown schema",
            ),
            (
                PreferencesError::Malformed,
                "the preferences file is not valid",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (mut editor, path) = blocked(&dir);
            editor.preferences_store = Store::Blocked {
                path: path.clone(),
                error,
                seen: None,
            };
            let secret = dir.path().to_string_lossy().into_owned();
            let status = save_status(
                editor,
                &format!("Applied for this session; not saved ({reason})"),
            );
            assert!(!status.contains(&secret));
        }
    }

    #[test]
    fn saving_with_a_session_only_store_says_it_was_not_saved() {
        save_status(
            Editor::default(),
            "Applied for this session; not saved (no config directory)",
        );
    }

    fn combos(output: &egui::FullOutput) -> Vec<(String, bool)> {
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        let mut found: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == accesskit::Role::ComboBox)
            .filter_map(|(_, n)| {
                let top = n.bounds()?.y0;
                Some((top, n.value()?.to_owned(), n.is_disabled()))
            })
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.into_iter().map(|(_, v, d)| (v, d)).collect()
    }

    fn text_shown(output: &egui::FullOutput, text: &str) -> bool {
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        update
            .nodes
            .iter()
            .any(|(_, n)| n.value() == Some(text) || n.label() == Some(text))
    }

    fn units_page(units: &Units) -> egui::FullOutput {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, SIZE)),
                ..Default::default()
            },
            |ui| working_units(ui, units),
        );
        output.textures_delta.clear();
        output
    }

    fn assert_units(units: Units, expected: [&str; 3]) {
        let output = units_page(&units);
        let found = combos(&output);
        let values: Vec<_> = found.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(values, expected);
        assert!(found.iter().all(|(_, disabled)| *disabled), "{found:?}");
        for label in ["Linear", "Angular", "Time", UNITS_REASON] {
            assert!(text_shown(&output, label), "{label}");
        }
    }

    #[test]
    fn working_units_show_a_meter_radian_ntsc_scene_read_only_with_the_reason() {
        assert_units(
            Units {
                linear: LinearUnit::Meter,
                angular: AngularUnit::Radian,
                time: "ntsc".into(),
            },
            ["meter", "radian", "ntsc"],
        );
    }

    #[test]
    fn working_units_show_an_unsupported_token_verbatim() {
        assert_units(
            Units {
                linear: LinearUnit::Unsupported("km".into()),
                angular: AngularUnit::Degree,
                time: "film".into(),
            },
            ["km", "degree", "film"],
        );
    }

    #[test]
    fn the_settings_page_shows_the_open_scenes_units() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        let output = settle(&mut editor, &ctx);
        let found = combos(&output);
        let found = found.into_iter().filter(|(_, d)| *d).collect::<Vec<_>>();
        let found = &found[found.len() - 3..];
        let values: Vec<_> = found.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(values, ["centimeter", "degree", "film"]);
        assert!(found.iter().all(|(_, disabled)| *disabled));
        assert!(text_shown(&output, UNITS_REASON));
    }

    #[test]
    fn the_settings_page_shows_a_loaded_meter_radian_ntsc_scene() {
        use binary_fixture::{chunk, group};
        let header = [
            chunk(b"LUNI", b"m", 8),
            chunk(b"AUNI", b"rad", 8),
            chunk(b"TUNI", b"ntsc", 8),
        ]
        .concat();
        let bytes = group(b"Maya", &group(b"HEAD", &header, 4), 1);
        let dir = tempfile::tempdir().unwrap();
        let scene = dir.path().join("units.mb");
        std::fs::write(&scene, bytes).unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        editor
            .apply_operation(Operation::OpenScene { path: scene })
            .unwrap();
        open_window(&mut editor, &ctx);
        let output = settle(&mut editor, &ctx);
        let found = combos(&output);
        let found = found.into_iter().filter(|(_, d)| *d).collect::<Vec<_>>();
        let values: Vec<_> = found[found.len() - 3..]
            .iter()
            .map(|(v, _)| v.as_str())
            .collect();
        assert_eq!(values, ["meter", "radian", "ntsc"]);
    }

    #[test]
    fn the_help_line_is_absent_at_the_documentation_pin() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        let output = settle(&mut editor, &ctx);
        assert!(!text_shown(&output, "Maya compatibility: 2027"));
    }

    const HELP_LINE_2015: &str = "Maya compatibility: 2015";

    fn pick(editor: &mut Editor, ctx: &egui::Context, year: u64) {
        let output = settle(editor, ctx);
        let current = editor
            .preferences_window
            .pending
            .unwrap_or(editor.preferences.maya_compatibility_version);
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        let combo = update
            .nodes
            .iter()
            .find(|(_, n)| {
                n.role() == accesskit::Role::ComboBox
                    && n.value()
                        .is_some_and(|text| text.starts_with(&current.year().to_string()))
            })
            .and_then(|(_, n)| n.bounds())
            .expect("compatibility picker");
        let at = Pos2::new(
            ((combo.x0 + combo.x1) / 2.0) as f32,
            ((combo.y0 + combo.y1) / 2.0) as f32,
        );
        click(editor, ctx, at);
        let mut output = settle(editor, ctx);
        if year > 2020 {
            // The popup list scrolls; bring the newest entries into view.
            let top = controls(&output)
                .into_iter()
                .find(|c| c.label == "2011")
                .expect("2011");
            run(editor, ctx, vec![Event::PointerMoved(top.rect.center())]);
            run(
                editor,
                ctx,
                vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -1000.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: Modifiers::NONE,
                }],
            );
            output = settle(editor, ctx);
        }
        let label = if year == 2027 {
            "2027 (documentation pin 2027.2)".to_owned()
        } else {
            year.to_string()
        };
        let item = controls(&output)
            .into_iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("picker entry {year}"));
        click(editor, ctx, item.rect.center());
        settle(editor, ctx);
    }

    #[test]
    fn picking_2015_then_save_persists_shows_the_help_line_and_closes() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = with_store(&dir);
        assert!(!text_shown(&settle(&mut editor, &ctx), HELP_LINE_2015));
        open_window(&mut editor, &ctx);
        pick(&mut editor, &ctx, 2015);
        assert_eq!(editor.preferences_window.pending, Some(v(2015)));
        assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
        assert!(!path.exists());
        press(&mut editor, &ctx, "Save");
        let output = settle(&mut editor, &ctx);
        assert!(!window_shown(&output));
        assert_eq!(editor.preferences.maya_compatibility_version, v(2015));
        assert!(std::fs::read_to_string(&path).unwrap().contains("2015"));
        assert!(text_shown(&output, HELP_LINE_2015));
    }

    #[test]
    fn picking_then_cancel_keeps_2027_and_writes_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = with_store(&dir);
        open_window(&mut editor, &ctx);
        pick(&mut editor, &ctx, 2015);
        assert_eq!(editor.preferences_window.pending, Some(v(2015)));
        press(&mut editor, &ctx, "Cancel");
        let output = settle(&mut editor, &ctx);
        assert!(!window_shown(&output));
        assert!(!text_shown(&output, HELP_LINE_2015));
        untouched(&editor, &path);
    }

    #[test]
    fn picking_the_current_value_again_leaves_nothing_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        pick(&mut editor, &ctx, 2015);
        pick(&mut editor, &ctx, 2027);
        assert_eq!(editor.preferences_window.pending, None);
    }

    #[test]
    fn a_save_error_keeps_the_picked_value_and_shows_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, b"x").unwrap();
        editor.preferences_store = Store::Writable {
            path: file.join("preferences.json"),
            baseline: Baseline::Missing,
        };
        open_window(&mut editor, &ctx);
        pick(&mut editor, &ctx, 2015);
        press(&mut editor, &ctx, "Save");
        let output = settle(&mut editor, &ctx);
        assert!(window_shown(&output));
        assert_eq!(editor.preferences_window.pending, Some(v(2015)));
        let error = editor.preferences_window.error.clone().expect("error kept");
        assert!(text_shown(&output, &error), "{error}");
        assert!(combos(&output).iter().any(|(text, _)| text == "2015"));
    }

    const RESET: &str = "Reset preferences file";
    const NOT_SAVED: &str = "Not saved: the preferences file is not valid";

    fn blocked_window(dir: &tempfile::TempDir) -> (Editor, egui::Context, PathBuf) {
        let (editor, path) = blocked(dir);
        let (editor, ctx) = setup(editor);
        (editor, ctx, path)
    }

    #[test]
    fn a_writable_store_says_saved_and_offers_no_reset() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = with_store(&dir);
        open_window(&mut editor, &ctx);
        let output = settle(&mut editor, &ctx);
        assert!(text_shown(&output, "Saved"));
        assert!(button(&output, RESET).is_none());
    }

    #[test]
    fn a_blocked_store_says_not_saved_and_offers_reset() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = blocked_window(&dir);
        open_window(&mut editor, &ctx);
        let output = settle(&mut editor, &ctx);
        assert!(text_shown(&output, NOT_SAVED));
        assert!(!text_shown(&output, "Saved"));
        assert!(!button(&output, RESET).expect("Reset button").disabled);
        assert!(!text_shown(&output, &dir.path().to_string_lossy()));
        assert_eq!(std::fs::read(path).unwrap(), b"not json");
    }

    #[test]
    fn reset_writes_the_backup_and_the_next_save_shows_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = blocked_window(&dir);
        open_window(&mut editor, &ctx);
        press(&mut editor, &ctx, RESET);
        let output = settle(&mut editor, &ctx);
        let backup = dir.path().join("preferences.json.bad-1");
        assert_eq!(std::fs::read(&backup).unwrap(), b"not json");
        assert!(!path.exists());
        assert!(button(&output, RESET).is_none());
        assert!(text_shown(&output, "Saved"));
        pick(&mut editor, &ctx, 2015);
        press(&mut editor, &ctx, "Save");
        assert!(std::fs::read_to_string(&path).unwrap().contains("2015"));
        assert_eq!(std::fs::read(&backup).unwrap(), b"not json");
        open_window(&mut editor, &ctx);
        assert!(text_shown(&settle(&mut editor, &ctx), "Saved"));
    }

    #[test]
    fn reset_is_disabled_while_a_value_is_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = blocked_window(&dir);
        open_window(&mut editor, &ctx);
        pick(&mut editor, &ctx, 2015);
        let output = settle(&mut editor, &ctx);
        assert!(button(&output, RESET).expect("Reset button").disabled);
        assert!(!dir.path().join("preferences.json.bad-1").exists());
        assert_eq!(std::fs::read(path).unwrap(), b"not json");
    }

    #[test]
    fn a_failed_reset_shows_its_error_in_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = blocked_window(&dir);
        open_window(&mut editor, &ctx);
        std::fs::write(&path, b"edited since").unwrap();
        press(&mut editor, &ctx, RESET);
        let output = settle(&mut editor, &ctx);
        let error = editor.preferences_window.error.clone().expect("error kept");
        assert!(error.contains("changed"), "{error}");
        assert!(text_shown(&output, &error), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"edited since");
    }

    #[test]
    fn a_successful_reset_replaces_a_stale_reset_error() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, path) = blocked_window(&dir);
        open_window(&mut editor, &ctx);
        std::fs::write(&path, b"edited since").unwrap();
        press(&mut editor, &ctx, RESET);
        settle(&mut editor, &ctx);
        assert!(editor.error);
        std::fs::write(&path, b"not json").unwrap();
        press(&mut editor, &ctx, RESET);
        settle(&mut editor, &ctx);
        assert!(!editor.error);
        assert_eq!(editor.status, "Preferences file reset");
        assert_eq!(editor.preferences_window.error, None);
        assert_eq!(
            std::fs::read(dir.path().join("preferences.json.bad-1")).unwrap(),
            b"not json"
        );
    }

    #[test]
    fn the_reset_pending_reason_is_visible_text_while_a_value_is_pending() {
        let dir = tempfile::tempdir().unwrap();
        let (mut editor, ctx, _) = blocked_window(&dir);
        open_window(&mut editor, &ctx);
        assert!(!text_shown(
            &settle(&mut editor, &ctx),
            RESET_PENDING_REASON
        ));
        pick(&mut editor, &ctx, 2015);
        assert!(text_shown(&settle(&mut editor, &ctx), RESET_PENDING_REASON));
    }
}
