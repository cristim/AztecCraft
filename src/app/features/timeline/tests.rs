use super::*;
use azteccraft::native_scene::{ALL_CHANNELS, MAX_FRAME};

fn cube() -> (Editor, NodeId) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    (editor, id)
}

fn set_tx(editor: &mut Editor, id: NodeId, value: f64) {
    editor
        .apply_operation(Operation::EditNode {
            id,
            name: None,
            translation: [Some(value), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
}

fn tx(editor: &Editor, id: NodeId) -> f64 {
    editor
        .history
        .scene()
        .node(id)
        .unwrap()
        .transform()
        .translation[0]
}

fn go(editor: &mut Editor, time: f64) -> Result<OperationReply, OperationError> {
    editor.apply_operation(Operation::SetCurrentTime { time })
}

fn key(editor: &mut Editor) -> Result<OperationReply, OperationError> {
    editor.apply_operation(Operation::SetKeyframe {
        nodes: None,
        time: None,
        channels: ALL_CHANNELS,
    })
}

/// Keys translate X at 0 on frame 1 and 24 on frame 25.
fn keyed() -> (Editor, NodeId) {
    let (mut editor, id) = cube();
    key(&mut editor).unwrap();
    go(&mut editor, 25.0).unwrap();
    set_tx(&mut editor, id, 24.0);
    key(&mut editor).unwrap();
    go(&mut editor, 1.0).unwrap();
    (editor, id)
}

fn undo_depth(editor: &mut Editor) -> usize {
    let mut depth = 0;
    while editor.history.undo() {
        depth += 1;
    }
    for _ in 0..depth {
        editor.history.redo();
    }
    depth
}

#[test]
fn the_workflow_keys_scrubs_and_undoes_through_the_shared_pipeline() {
    let (mut editor, id) = keyed();
    assert_eq!(tx(&editor, id), 0.0);
    assert!(matches!(go(&mut editor, 13.0), Ok(OperationReply::Changed)));
    assert_eq!(tx(&editor, id), 12.0);
    assert_eq!(editor.channels[0], "12");
    assert!(!editor.has_draft());
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene().session_key_count(), 9);
    // Undo re-evaluates at the current time, which it does not restore (PROVISIONAL).
    assert_eq!(editor.timeline.current, 13);
    assert_eq!(tx(&editor, id), 0.0);
    assert!(!editor.has_draft());
    editor.apply_operation(Operation::Redo).unwrap();
    assert_eq!(tx(&editor, id), 12.0);
}

#[test]
fn a_time_change_is_no_undo_step_and_does_not_dirty_a_scene_without_keys() {
    let mut editor = Editor::default();
    let (revision, undo) = (editor.history.revision(), editor.history.can_undo());
    go(&mut editor, 30.0).unwrap();
    assert_eq!(editor.timeline.current, 30);
    assert_eq!(editor.history.revision(), revision);
    assert_eq!(editor.history.can_undo(), undo);
    assert!(!editor.is_dirty());
}

#[test]
fn scrubbing_keeps_the_undo_and_redo_stacks() {
    let (mut editor, _) = keyed();
    let depth = undo_depth(&mut editor);
    editor.apply_operation(Operation::Undo).unwrap();
    assert!(editor.history.can_redo());
    for frame in 2..20 {
        go(&mut editor, f64::from(frame)).unwrap();
    }
    assert!(editor.history.can_redo());
    assert_eq!(undo_depth(&mut editor), depth - 1);
}

#[test]
fn the_same_time_is_unchanged_and_leaves_a_held_edit() {
    let (mut editor, id) = keyed();
    set_tx(&mut editor, id, 50.0);
    assert!(matches!(
        go(&mut editor, 1.0),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(tx(&editor, id), 50.0);
    go(&mut editor, 2.0).unwrap();
    assert_eq!(tx(&editor, id), 1.0);
}

#[test]
fn a_draft_refuses_a_time_change_without_writing_or_committing() {
    let (mut editor, id) = keyed();
    editor.channels[0] = "77".into();
    assert!(editor.has_draft());
    let revision = editor.history.revision();
    assert!(matches!(
        go(&mut editor, 13.0),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.channels[0], "77");
    assert_eq!(editor.timeline.current, 1);
    assert_eq!(editor.history.revision(), revision);
    assert_eq!(tx(&editor, id), 0.0);
}

#[test]
fn time_validation_runs_before_the_draft_check_and_changes_nothing() {
    let (mut editor, _) = keyed();
    editor.channels[0] = "77".into();
    for time in [1.5, f64::NAN, f64::INFINITY, f64::from(MAX_FRAME) + 1.0] {
        assert!(matches!(
            go(&mut editor, time),
            Err(OperationError::Scene(NativeError::InvalidFrame))
        ));
    }
    assert_eq!(editor.channels[0], "77");
    assert_eq!(editor.timeline.current, 1);
}

#[test]
fn a_key_commits_a_valid_draft_first_like_other_native_actions() {
    let (mut editor, id) = cube();
    editor.channels[0] = "3".into();
    key(&mut editor).unwrap();
    assert_eq!(tx(&editor, id), 3.0);
    assert_eq!(editor.history.scene().session_key_count(), 9);
}

#[test]
fn a_refused_key_never_commits_a_draft() {
    let (mut editor, id) = cube();
    editor.channels[0] = "3".into();
    assert!(matches!(
        editor.apply_operation(Operation::SetKeyframe {
            nodes: None,
            time: Some(0.5),
            channels: ALL_CHANNELS,
        }),
        Err(OperationError::Scene(NativeError::InvalidFrame))
    ));
    assert_eq!(tx(&editor, id), 0.0);
    assert_eq!(editor.channels[0], "3");
}

#[test]
fn a_refused_playback_range_never_commits_a_draft() {
    let (mut editor, id) = cube();
    editor.channels[0] = "3".into();
    assert!(matches!(
        editor.apply_operation(Operation::SetPlaybackRange {
            start: 12.0,
            end: 2.0
        }),
        Err(OperationError::Scene(NativeError::InvalidPlaybackRange))
    ));
    assert!(
        editor
            .apply_operation(Operation::SetPlaybackRange {
                start: 1.5,
                end: 2.0
            })
            .is_err()
    );
    assert_eq!(tx(&editor, id), 0.0);
    assert_eq!(editor.channels[0], "3");
}

#[test]
fn keying_the_selection_needs_one_and_an_object_mode() {
    let mut editor = Editor::default();
    assert!(matches!(
        key(&mut editor),
        Err(OperationError::InvalidInput(message)) if message == "Select an object first"
    ));
    let (mut editor, _) = cube();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    assert!(matches!(
        key(&mut editor),
        Err(OperationError::InvalidInput(message))
            if message == "Keys need Object or Hierarchy selection mode"
    ));
    assert_eq!(editor.history.scene().session_key_count(), 0);
}

#[test]
fn a_request_with_no_nodes_or_channels_is_refused() {
    let (mut editor, _) = cube();
    for (nodes, channels) in [(Some(Vec::new()), ALL_CHANNELS), (None, [false; 9])] {
        assert!(matches!(
            editor.apply_operation(Operation::SetKeyframe {
                nodes,
                time: None,
                channels,
            }),
            Err(OperationError::InvalidInput(_))
        ));
    }
}

#[test]
fn keying_several_objects_is_one_undo_step() {
    let (mut editor, first) = cube();
    editor.create(Primitive::Cube);
    let second = editor.selected().unwrap();
    editor
        .apply_operation(Operation::ModifySelection {
            change: SelectionChange::Add,
            node: first,
            instance_path: None,
        })
        .unwrap();
    assert_eq!(editor.selection.len(), 2);
    let depth = undo_depth(&mut editor);
    key(&mut editor).unwrap();
    assert_eq!(editor.history.scene().session_key_count(), 18);
    assert_eq!(undo_depth(&mut editor), depth + 1);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Set Key"));
    let scene = editor.history.scene();
    assert!(!scene.key_frames(&[first]).is_empty() && !scene.key_frames(&[second]).is_empty());
}

#[test]
fn delete_prunes_keys_and_undo_brings_them_back_evaluated() {
    let (mut editor, id) = keyed();
    go(&mut editor, 13.0).unwrap();
    editor.apply_operation(Operation::Delete(id)).unwrap();
    assert_eq!(editor.history.scene().session_key_count(), 0);
    go(&mut editor, 25.0).unwrap();
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene().session_key_count(), 18);
    assert_eq!(tx(&editor, id), 24.0);
}

#[test]
fn deleting_all_keys_leaves_the_evaluated_pose() {
    let (mut editor, id) = keyed();
    go(&mut editor, 13.0).unwrap();
    editor
        .apply_operation(Operation::DeleteKeys {
            nodes: None,
            time: None,
            channels: ALL_CHANNELS,
        })
        .unwrap();
    assert_eq!(editor.history.scene().session_key_count(), 0);
    assert_eq!(tx(&editor, id), 12.0);
    go(&mut editor, 1.0).unwrap();
    assert_eq!(tx(&editor, id), 12.0);
}

#[test]
fn a_key_on_another_frame_keeps_the_pose_of_the_current_frame() {
    let (mut editor, id) = keyed();
    go(&mut editor, 13.0).unwrap();
    set_tx(&mut editor, id, 500.0);
    editor
        .apply_operation(Operation::SetKeyframe {
            nodes: Some(vec![id]),
            time: Some(40.0),
            channels: ALL_CHANNELS,
        })
        .unwrap();
    assert_eq!(tx(&editor, id), 12.0);
    assert_eq!(editor.channels[0], "12");
    assert!(!editor.has_draft());
}

#[test]
fn a_manipulation_refuses_a_time_change() {
    let (mut editor, id) = keyed();
    let path = vec![editor.history.scene().node(id).unwrap().transform_id()];
    editor
        .apply_operation(Operation::BeginMove {
            target: TransformTarget { node: id, path },
            axis: azteccraft::native_scene::WorldAxis::X,
        })
        .unwrap();
    assert!(matches!(
        go(&mut editor, 13.0),
        Err(OperationError::BusyManipulation)
    ));
    assert_eq!(editor.timeline.current, 1);
}

#[test]
fn the_playback_range_is_validated_and_is_not_an_undo_step() {
    let (mut editor, _) = cube();
    let range = |start, end| Operation::SetPlaybackRange { start, end };
    assert!(matches!(
        editor.apply_operation(range(12.0, 2.0)),
        Err(OperationError::Scene(NativeError::InvalidPlaybackRange))
    ));
    assert!(matches!(
        editor.apply_operation(range(1.5, 2.0)),
        Err(OperationError::Scene(NativeError::InvalidFrame))
    ));
    assert_eq!((editor.timeline.start, editor.timeline.end), (1, 48));
    let depth = undo_depth(&mut editor);
    assert!(matches!(
        editor.apply_operation(range(2.0, 12.0)),
        Ok(OperationReply::Changed)
    ));
    assert!(matches!(
        editor.apply_operation(range(2.0, 12.0)),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(undo_depth(&mut editor), depth);
}

#[test]
fn playback_advances_at_24_fps_loops_and_fills_no_undo_queue() {
    let (mut editor, id) = keyed();
    editor
        .apply_operation(Operation::SetPlaybackRange {
            start: 1.0,
            end: 4.0,
        })
        .unwrap();
    editor.apply_operation(Operation::Play).unwrap();
    let depth = undo_depth(&mut editor);
    assert!(editor.timeline.playing);
    editor.tick_playback(10.0, false);
    assert_eq!(editor.timeline.current, 1);
    editor.tick_playback(10.0 + 1.5 / 24.0, false);
    assert_eq!(editor.timeline.current, 2);
    assert_eq!(tx(&editor, id), 1.0);
    editor.tick_playback(10.0 + 3.5 / 24.0, false);
    assert_eq!(editor.timeline.current, 4);
    editor.tick_playback(10.0 + 4.5 / 24.0, false);
    assert_eq!(editor.timeline.current, 1);
    // A late tick skips frames rather than slowing down.
    editor.tick_playback(10.0 + 42.5 / 24.0, false);
    assert_eq!(editor.timeline.current, 3);
    assert_eq!(undo_depth(&mut editor), depth);
    assert!(editor.timeline.playing);
}

#[test]
fn a_clock_that_runs_back_restarts_the_count_without_a_jump() {
    let (mut editor, _) = keyed();
    editor.apply_operation(Operation::Play).unwrap();
    editor.tick_playback(10.0, false);
    editor.tick_playback(11.0, false);
    assert_eq!(editor.timeline.current, 25);
    editor.tick_playback(2.0, false);
    assert_eq!(editor.timeline.current, 25);
    assert!(editor.timeline.playing);
    editor.tick_playback(2.0 + 1.5 / 24.0, false);
    assert_eq!(editor.timeline.current, 26);
    editor.tick_playback(f64::NAN, false);
    assert_eq!(editor.timeline.current, 26);
}

#[test]
fn play_starts_at_the_range_start_when_the_time_is_outside_it() {
    let (mut editor, id) = keyed();
    go(&mut editor, 100.0).unwrap();
    editor.apply_operation(Operation::Play).unwrap();
    assert_eq!(editor.timeline.current, 1);
    assert_eq!(tx(&editor, id), 0.0);
}

#[test]
fn a_single_frame_range_stays_on_its_frame() {
    let (mut editor, _) = keyed();
    editor
        .apply_operation(Operation::SetPlaybackRange {
            start: 7.0,
            end: 7.0,
        })
        .unwrap();
    editor.apply_operation(Operation::Play).unwrap();
    assert_eq!(editor.timeline.current, 7);
    editor.tick_playback(1.0, false);
    editor.tick_playback(90.0, false);
    assert_eq!(editor.timeline.current, 7);
}

#[test]
fn a_draft_refuses_play_and_stop_never_commits_one() {
    let (mut editor, id) = keyed();
    editor.channels[0] = "77".into();
    assert!(matches!(
        editor.apply_operation(Operation::Play),
        Err(OperationError::BusyEditing)
    ));
    assert!(!editor.timeline.playing);
    assert!(matches!(
        editor.apply_operation(Operation::Stop),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.channels[0], "77");
    assert_eq!(tx(&editor, id), 0.0);
}

#[test]
fn playback_stops_on_a_draft_without_committing_or_writing_it() {
    let (mut editor, id) = keyed();
    editor.apply_operation(Operation::Play).unwrap();
    editor.tick_playback(1.0, false);
    editor.channels[0] = "77".into();
    let revision = editor.history.revision();
    editor.tick_playback(2.0, false);
    assert!(!editor.timeline.playing);
    assert_eq!(editor.channels[0], "77");
    assert_eq!(editor.history.revision(), revision);
    assert_eq!(tx(&editor, id), 0.0);
    assert_eq!(editor.status, "Playback stopped: a field holds a draft");
}

#[test]
fn playback_stops_on_a_focused_field_a_manipulation_and_a_file_modal() {
    let (mut editor, _) = keyed();
    editor.apply_operation(Operation::Play).unwrap();
    editor.tick_playback(1.0, true);
    assert!(!editor.timeline.playing);

    let (mut editor, id) = keyed();
    editor.apply_operation(Operation::Play).unwrap();
    let path = vec![editor.history.scene().node(id).unwrap().transform_id()];
    editor
        .apply_operation(Operation::BeginMove {
            target: TransformTarget { node: id, path },
            axis: azteccraft::native_scene::WorldAxis::X,
        })
        .unwrap();
    // Beginning the move already stopped playback, and a remote manipulation cannot restart it.
    assert!(!editor.timeline.playing);
    assert!(matches!(
        editor.apply_operation(Operation::Play),
        Err(OperationError::BusyManipulation)
    ));

    let (mut editor, _) = keyed();
    editor.apply_operation(Operation::Play).unwrap();
    editor.timeline.anchor = None;
    assert!(matches!(
        editor.apply_operation(Operation::NewScene),
        Ok(OperationReply::PendingConfirmation { .. })
    ));
    assert!(editor.files.is_modal());
    assert!(!editor.timeline.playing);
    assert!(matches!(
        editor.apply_operation(Operation::Play),
        Err(OperationError::BusyEditing)
    ));
}

#[test]
fn mutating_operations_stop_playback_and_view_operations_do_not() {
    let (mut editor, id) = keyed();
    editor.apply_operation(Operation::Play).unwrap();
    editor
        .apply_operation(Operation::SetGridVisible { visible: false })
        .unwrap();
    editor
        .apply_operation(Operation::Select {
            node: Some(id),
            instance_path: None,
        })
        .unwrap();
    assert!(editor.timeline.playing);
    key(&mut editor).unwrap();
    assert!(!editor.timeline.playing);

    editor.apply_operation(Operation::Play).unwrap();
    go(&mut editor, 3.0).unwrap();
    assert!(!editor.timeline.playing);

    editor.apply_operation(Operation::Play).unwrap();
    editor.apply_operation(Operation::Undo).unwrap();
    assert!(!editor.timeline.playing);
}

#[test]
fn new_resets_the_timeline() {
    let (mut editor, _) = keyed();
    editor
        .apply_operation(Operation::SetPlaybackRange {
            start: 2.0,
            end: 12.0,
        })
        .unwrap();
    go(&mut editor, 9.0).unwrap();
    editor.apply_operation(Operation::Play).unwrap();
    let pending = editor.apply_operation(Operation::NewScene).unwrap();
    let OperationReply::PendingConfirmation {
        pending_action_id, ..
    } = pending
    else {
        panic!("{pending:?}");
    };
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: pending_action_id.parse().unwrap(),
            decision: crate::editor_operations::FileDecision::Discard,
        })
        .unwrap();
    let timeline = &editor.timeline;
    assert_eq!(
        (
            timeline.current,
            timeline.start,
            timeline.end,
            timeline.playing
        ),
        (1, 1, 48, false)
    );
    assert_eq!(editor.history.scene().session_key_count(), 0);
}

#[test]
fn a_scene_with_keys_cannot_be_saved_and_stays_dirty() {
    let (mut editor, _) = keyed();
    let result = editor.apply_operation(Operation::SaveScene);
    assert!(matches!(
        result,
        Err(OperationError::NativeSave(
            azteccraft::document::NativeSaveError::Unsupported
        ))
    ));
    assert!(editor.is_dirty());
}

#[test]
fn a_page_of_keys_ends_on_the_limit_and_its_cursor_binds_the_revision() {
    use crate::editor_operations::{MAX_KEYS_PER_PAGE, PageCursor};
    let (mut editor, id) = cube();
    for frame in 0..=(MAX_KEYS_PER_PAGE / 9) as i32 {
        editor
            .apply_operation(Operation::SetKeyframe {
                nodes: Some(vec![id]),
                time: Some(f64::from(frame)),
                channels: ALL_CHANNELS,
            })
            .unwrap();
    }
    let first = editor.inspect_keys(None, None).unwrap();
    assert_eq!(first.keys.len(), MAX_KEYS_PER_PAGE);
    assert_eq!(first.keys_total, (MAX_KEYS_PER_PAGE / 9 + 1) * 9);
    let cursor: PageCursor = first.next_cursor.unwrap().parse().unwrap();
    let last = editor.inspect_keys(Some(id), Some(&cursor)).unwrap();
    assert_eq!(last.key_offset, MAX_KEYS_PER_PAGE);
    assert_eq!(last.keys.len(), first.keys_total - MAX_KEYS_PER_PAGE);
    assert_eq!(last.next_cursor, None);
    let past = PageCursor {
        offset: first.keys_total + 1,
        ..cursor
    };
    assert!(matches!(
        editor.inspect_keys(None, Some(&past)),
        Err(OperationError::InvalidInput(_))
    ));
    // Any write makes the cursor stale, and a time change at another frame is a write.
    set_tx(&mut editor, id, 5.0);
    go(&mut editor, 3.0).unwrap();
    assert!(matches!(
        editor.inspect_keys(None, Some(&cursor)),
        Err(OperationError::StaleScene)
    ));
    assert!(matches!(
        editor.inspect_keys(Some(NodeId::from_decimal("999").unwrap()), None),
        Err(OperationError::Scene(_))
    ));
}

fn press(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A press and its release: egui marks a second press of a held key as a repeat.
fn tap(editor: &mut Editor, context: &egui::Context, key: Key, modifiers: egui::Modifiers) {
    let release = egui::Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers,
    };
    run(editor, context, vec![press(key, modifiers), release]);
}

fn run(editor: &mut Editor, context: &egui::Context, events: Vec<egui::Event>) {
    crate::app::tests::frame_with(editor, context, egui::vec2(1440.0, 900.0), 1.0, events);
}

#[test]
fn the_hotkeys_map_to_the_timeline_actions() {
    let alt = egui::Modifiers::ALT;
    let none = egui::Modifiers::NONE;
    for (event, action) in [
        (press(Key::S, none), Some(TimelineAction::SetKey)),
        (press(Key::V, alt), Some(TimelineAction::TogglePlay)),
        (press(Key::Comma, alt), Some(TimelineAction::StepFrame(-1))),
        (press(Key::Period, alt), Some(TimelineAction::StepFrame(1))),
        (press(Key::Comma, none), Some(TimelineAction::PreviousKey)),
        (press(Key::Period, none), Some(TimelineAction::NextKey)),
        (press(Key::S, egui::Modifiers::SHIFT), None),
        (press(Key::S, egui::Modifiers::CTRL), None),
        (press(Key::V, none), None),
        (press(Key::V, alt | egui::Modifiers::SHIFT), None),
        (press(Key::Comma, egui::Modifiers::CTRL), None),
    ] {
        assert_eq!(
            timeline_chord(std::slice::from_ref(&event)),
            action,
            "{event:?}"
        );
    }
}

#[test]
fn s_does_not_repeat_but_a_held_step_does() {
    let repeated = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: true,
        modifiers,
    };
    assert_eq!(
        timeline_chord(&[repeated(Key::S, egui::Modifiers::NONE)]),
        None
    );
    assert_eq!(
        timeline_chord(&[repeated(Key::V, egui::Modifiers::ALT)]),
        None
    );
    assert_eq!(
        timeline_chord(&[repeated(Key::Comma, egui::Modifiers::ALT)]),
        Some(TimelineAction::StepFrame(-1))
    );
}

#[test]
fn s_keys_the_selection_and_reports_a_refusal() {
    let context = egui::Context::default();
    let (mut editor, _) = cube();
    run(&mut editor, &context, Vec::new());
    tap(&mut editor, &context, Key::S, egui::Modifiers::NONE);
    assert_eq!(editor.history.scene().session_key_count(), 9);
    assert_eq!(editor.status, "Key set");

    let mut empty = Editor::default();
    tap(&mut empty, &context, Key::S, egui::Modifiers::NONE);
    assert_eq!(empty.status, "Select an object first");
    assert!(empty.error);
}

#[test]
fn stepping_moves_by_frame_and_by_selected_key() {
    let context = egui::Context::default();
    let (mut editor, _) = keyed();
    run(&mut editor, &context, Vec::new());
    tap(&mut editor, &context, Key::Period, egui::Modifiers::ALT);
    assert_eq!(editor.timeline.current, 2);
    tap(&mut editor, &context, Key::Comma, egui::Modifiers::ALT);
    assert_eq!(editor.timeline.current, 1);
    tap(&mut editor, &context, Key::Period, egui::Modifiers::NONE);
    assert_eq!(editor.timeline.current, 25);
    tap(&mut editor, &context, Key::Period, egui::Modifiers::NONE);
    assert_eq!(editor.timeline.current, 25);
    assert_eq!(editor.status, "No next key on the selection");
    tap(&mut editor, &context, Key::Comma, egui::Modifiers::NONE);
    assert_eq!(editor.timeline.current, 1);
}

#[test]
fn a_step_key_ignores_keys_of_unselected_objects() {
    let (mut editor, _) = keyed();
    editor
        .apply_operation(Operation::Select {
            node: None,
            instance_path: None,
        })
        .unwrap();
    editor.run_timeline_action(TimelineAction::NextKey);
    assert_eq!(editor.timeline.current, 1);
    assert_eq!(editor.status, "No next key on the selection");
}

#[test]
fn alt_v_toggles_and_esc_stops_playback_before_its_other_meanings() {
    let context = egui::Context::default();
    let (mut editor, _) = keyed();
    run(&mut editor, &context, Vec::new());
    tap(&mut editor, &context, Key::V, egui::Modifiers::ALT);
    assert!(editor.timeline.playing);
    tap(&mut editor, &context, Key::V, egui::Modifiers::ALT);
    assert!(!editor.timeline.playing);

    editor.run_timeline_action(TimelineAction::TogglePlay);
    tap(&mut editor, &context, Key::Escape, egui::Modifiers::NONE);
    assert!(!editor.timeline.playing);
    assert_eq!(editor.status, "Playback stopped");

    // With nothing playing, Esc keeps its old meaning.
    tap(&mut editor, &context, Key::Escape, egui::Modifiers::NONE);
    assert_eq!(editor.status, "Edit cancelled");
}

#[test]
fn esc_stops_playback_and_leaves_an_open_options_window() {
    let context = egui::Context::default();
    let (mut editor, _) = keyed();
    editor.open_primitive_options(Primitive::Cube);
    editor.run_timeline_action(TimelineAction::TogglePlay);
    run(&mut editor, &context, Vec::new());
    tap(&mut editor, &context, Key::Escape, egui::Modifiers::NONE);
    assert!(!editor.timeline.playing);
    // The window is still open: the next Esc is the one that closes it.
    run(&mut editor, &context, vec![]);
    let esc = egui::RawInput {
        events: vec![press(Key::Escape, egui::Modifiers::NONE)],
        ..Default::default()
    };
    let mut closed = false;
    let mut output = context.run_ui(esc, |ui| {
        closed = editor.escape_closes_primitive_options(ui.ctx());
    });
    output.textures_delta.clear();
    assert!(closed);
}

fn slider_point(context: &egui::Context, fraction: f32) -> egui::Pos2 {
    let rect = context.read_response(time_slider_id()).unwrap().rect;
    egui::pos2(
        rect.left() + 10.0 + (rect.width() - 20.0) * fraction,
        rect.center().y,
    )
}

/// The bottom panel settles over its first frames, so the slider rect is read after them.
fn settle(editor: &mut Editor, context: &egui::Context) {
    for _ in 0..3 {
        run(editor, context, Vec::new());
    }
}

fn slider_frame(editor: &mut Editor, context: &egui::Context, fraction: f32) {
    settle(editor, context);
    let at = slider_point(context, fraction);
    let mut output = crate::app::tests::click_at(editor, context, egui::vec2(1440.0, 900.0), at);
    output.textures_delta.clear();
}

#[test]
fn a_click_on_the_time_slider_moves_the_time_and_writes_the_pose() {
    let context = egui::Context::default();
    let (mut editor, id) = keyed();
    editor
        .apply_operation(Operation::SetPlaybackRange {
            start: 1.0,
            end: 25.0,
        })
        .unwrap();
    slider_frame(&mut editor, &context, 0.5);
    assert_eq!(editor.timeline.current, 13);
    assert_eq!(tx(&editor, id), 12.0);
    assert_eq!(editor.channels[0], "12");
    assert!(!editor.has_draft());
}

#[test]
fn a_drag_on_the_time_slider_scrubs_with_no_undo_entry() {
    let context = egui::Context::default();
    let (mut editor, id) = keyed();
    editor
        .apply_operation(Operation::SetPlaybackRange {
            start: 1.0,
            end: 25.0,
        })
        .unwrap();
    let depth = undo_depth(&mut editor);
    settle(&mut editor, &context);
    let from = slider_point(&context, 0.25);
    let to = slider_point(&context, 0.75);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    run(&mut editor, &context, vec![egui::Event::PointerMoved(from)]);
    run(&mut editor, &context, vec![button(from, true)]);
    assert_eq!(editor.timeline.current, 7);
    run(&mut editor, &context, vec![egui::Event::PointerMoved(to)]);
    run(&mut editor, &context, vec![egui::Event::PointerMoved(to)]);
    assert_eq!(editor.timeline.current, 19);
    assert_eq!(tx(&editor, id), 18.0);
    run(&mut editor, &context, vec![button(to, false)]);
    assert_eq!(undo_depth(&mut editor), depth);
}

#[test]
fn the_time_slider_does_nothing_under_a_file_modal() {
    let context = egui::Context::default();
    let (mut editor, _) = keyed();
    editor.apply_operation(Operation::NewScene).unwrap();
    assert!(editor.files.is_modal());
    slider_frame(&mut editor, &context, 0.5);
    assert_eq!(editor.timeline.current, 1);
}

#[test]
fn a_time_field_acts_on_leaving_it_and_esc_abandons_the_text() {
    let context = egui::Context::default();
    let (mut editor, id) = keyed();
    run(&mut editor, &context, Vec::new());
    assert_eq!(editor.timeline.time_field.text, "1");
    context.memory_mut(|memory| memory.request_focus(time_field_id()));
    run(&mut editor, &context, Vec::new());
    assert!(text_edit_focused(&context));
    editor.timeline.time_field.text = "13".into();
    run(&mut editor, &context, Vec::new());
    assert_eq!(editor.timeline.current, 1);
    tap(&mut editor, &context, Key::Enter, egui::Modifiers::NONE);
    assert_eq!(editor.timeline.current, 13);
    assert_eq!(tx(&editor, id), 12.0);

    context.memory_mut(|memory| memory.request_focus(time_field_id()));
    run(&mut editor, &context, Vec::new());
    editor.timeline.time_field.text = "20".into();
    tap(&mut editor, &context, Key::Escape, egui::Modifiers::NONE);
    run(&mut editor, &context, Vec::new());
    assert_eq!(editor.timeline.current, 13);
    assert_eq!(editor.timeline.time_field.text, "13");
}

#[test]
fn a_bad_time_field_text_is_refused_and_the_field_recovers() {
    let context = egui::Context::default();
    let (mut editor, _) = keyed();
    run(&mut editor, &context, Vec::new());
    for text in ["soon", "1.5", "NaN", "inf"] {
        context.memory_mut(|memory| memory.request_focus(time_field_id()));
        run(&mut editor, &context, Vec::new());
        editor.timeline.time_field.text = text.into();
        tap(&mut editor, &context, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(editor.status, "Enter a whole frame", "{text}");
        assert!(editor.error);
        run(&mut editor, &context, Vec::new());
        assert_eq!(editor.timeline.current, 1);
        assert_eq!(editor.timeline.time_field.text, "1");
    }
}

#[test]
fn the_range_fields_set_the_playback_range_and_refuse_an_inverted_one() {
    let context = egui::Context::default();
    let (mut editor, _) = keyed();
    run(&mut editor, &context, Vec::new());
    context.memory_mut(|memory| memory.request_focus(end_field_id()));
    run(&mut editor, &context, Vec::new());
    editor.timeline.end_field.text = "30".into();
    tap(&mut editor, &context, Key::Enter, egui::Modifiers::NONE);
    assert_eq!((editor.timeline.start, editor.timeline.end), (1, 30));

    context.memory_mut(|memory| memory.request_focus(start_field_id()));
    run(&mut editor, &context, Vec::new());
    editor.timeline.start_field.text = "99".into();
    tap(&mut editor, &context, Key::Enter, egui::Modifiers::NONE);
    assert_eq!((editor.timeline.start, editor.timeline.end), (1, 30));
    assert!(editor.error);
}

#[test]
fn a_focused_field_stops_playback_in_the_real_frame_loop() {
    let context = egui::Context::default();
    let (mut editor, id) = keyed();
    editor.run_timeline_action(TimelineAction::TogglePlay);
    run(&mut editor, &context, Vec::new());
    assert!(editor.timeline.playing);
    context.memory_mut(|memory| memory.request_focus(name_edit_id()));
    run(&mut editor, &context, Vec::new());
    assert!(!editor.timeline.playing);
    assert_eq!(editor.status, "Playback stopped: a text field is focused");
    assert_eq!(tx(&editor, id), 0.0);
}

#[test]
fn a_hidden_window_stops_playback() {
    let (mut editor, _) = keyed();
    editor.run_timeline_action(TimelineAction::TogglePlay);
    let context = egui::Context::default();
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .minimized = Some(true);
    let mut native_frame = eframe::Frame::_new_kittest();
    let mut output = context.run_ui(input, |ui| {
        eframe::App::logic(&mut editor, ui.ctx(), &mut native_frame);
    });
    output.textures_delta.clear();
    assert!(!editor.timeline.playing);
}

#[test]
fn the_ruler_step_keeps_room_for_labels_and_stays_bounded() {
    assert_eq!(ruler_step(47.0, 1000.0), 2);
    assert_eq!(ruler_step(4.0, 1000.0), 1);
    assert_eq!(ruler_step(2_000_000.0, 1000.0), 100_000);
    assert!(ruler_step(2_000_000.0, 0.001) > 0);
}

#[test]
fn ruler_labels_near_the_right_edge_stay_inside_it() {
    let longest = MAX_FRAME.to_string().len() as f32 * 7.0;
    let (x, align) = ruler_label_anchor(1000.0 - longest, 1000.0);
    assert_eq!(align, egui::Align2::RIGHT_TOP);
    assert!(x < 1000.0);
    let (x, align) = ruler_label_anchor(100.0, 1000.0);
    assert_eq!((x, align), (102.0, egui::Align2::LEFT_TOP));
}
