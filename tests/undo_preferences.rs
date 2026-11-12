use azteccraft::history::{Command, EditOutcome, History, UNDO_LIMIT_MAX, UndoQueue};
use azteccraft::native_scene::NativeError;
use azteccraft::scene::{NodeId, Primitive};

fn create(history: &mut History, name: &str) -> NodeId {
    match history
        .execute(Command::Create {
            primitive: Primitive::Cube,
            options: None,
            name: name.into(),
        })
        .unwrap()
    {
        EditOutcome::Created(id) => id,
        outcome => panic!("unexpected outcome: {outcome:?}"),
    }
}

fn rename(history: &mut History, id: NodeId, n: usize) {
    history
        .execute(Command::Rename {
            id,
            name: format!("n{n}"),
        })
        .unwrap();
}

fn node_count(history: &History) -> usize {
    history.scene().nodes().len()
}

#[test]
fn the_default_queue_is_finite_fifty_and_undo_is_on() {
    let history = History::default();
    assert_eq!(history.undo_queue(), UndoQueue::Finite(50));
    assert!(history.undo_enabled());
}

#[test]
fn finite_size_must_be_in_range() {
    assert!(UndoQueue::finite(0).is_err());
    assert!(UndoQueue::finite(UNDO_LIMIT_MAX + 1).is_err());
    assert_eq!(UndoQueue::finite(1).unwrap(), UndoQueue::Finite(1));
    let mut history = History::default();
    assert!(history.set_undo_queue(UndoQueue::Finite(0)).is_err());
    assert_eq!(history.undo_queue(), UndoQueue::Finite(50));
}

#[test]
fn a_finite_size_caps_the_queue_and_a_smaller_size_trims_the_oldest() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    history.set_undo_queue(UndoQueue::Finite(3)).unwrap();
    for n in 0..10 {
        rename(&mut history, id, n);
    }
    assert_eq!(history.undo_len(), 3);
    history.set_undo_queue(UndoQueue::Finite(2)).unwrap();
    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.undo_label().as_deref(), Some("Rename"));
    history.set_undo_queue(UndoQueue::Finite(10)).unwrap();
    assert_eq!(history.undo_len(), 2);
}

#[test]
fn infinite_keeps_more_than_the_default_limit() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    history.set_undo_queue(UndoQueue::Infinite).unwrap();
    for n in 0..120 {
        rename(&mut history, id, n);
    }
    assert_eq!(history.undo_len(), 120 + 1);
    let mut undone = 0;
    while history.undo() {
        undone += 1;
    }
    assert_eq!(undone, 121);
    assert_eq!(node_count(&history), 0);
}

#[test]
fn a_disabled_queue_applies_edits_records_nothing_and_flushes_stale_entries() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    rename(&mut history, id, 1);
    history.undo();
    assert!(history.can_undo() && history.can_redo());
    history.set_undo_enabled(false, false).unwrap();
    assert!(!history.undo() && !history.redo());
    assert!(history.can_undo(), "toggling without flush keeps the queue");
    create(&mut history, "b");
    assert_eq!(node_count(&history), 2);
    assert!(!history.can_undo() && !history.can_redo());
    history.set_undo_enabled(true, false).unwrap();
    assert!(
        !history.undo(),
        "no stale snapshot can erase the unrecorded edit"
    );
    create(&mut history, "c");
    assert!(history.undo());
    assert_eq!(node_count(&history), 2);
}

#[test]
fn turning_undo_off_with_flush_clears_both_stacks() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    rename(&mut history, id, 1);
    history.undo();
    history.set_undo_enabled(false, true).unwrap();
    assert_eq!((history.undo_len(), history.redo_len()), (0, 0));
}

#[test]
fn a_chunk_groups_edits_into_one_named_undo_step() {
    let mut history = History::default();
    create(&mut history, "base");
    history.open_chunk("Build").unwrap();
    assert_eq!(history.open_chunk_name(), Some("Build"));
    create(&mut history, "b");
    create(&mut history, "c");
    assert_eq!(history.undo_len(), 1, "nothing recorded while open");
    assert_eq!(history.close_chunk().unwrap(), EditOutcome::Changed);
    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.undo_label().as_deref(), Some("Build"));
    assert!(history.undo());
    assert_eq!(node_count(&history), 1);
    assert!(history.redo());
    assert_eq!(node_count(&history), 3);
    assert_eq!(history.open_chunk_name(), None);
}

#[test]
fn an_empty_chunk_records_nothing_and_an_unnamed_one_is_called_chunk() {
    let mut history = History::default();
    history.open_chunk("x").unwrap();
    assert_eq!(history.close_chunk().unwrap(), EditOutcome::Unchanged);
    assert!(!history.can_undo());
    history.open_chunk("  ").unwrap();
    create(&mut history, "a");
    history.close_chunk().unwrap();
    assert_eq!(history.undo_label().as_deref(), Some("Chunk"));
}

#[test]
fn a_chunk_whose_edits_cancel_out_records_no_step() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    history.open_chunk("Round trip").unwrap();
    rename(&mut history, id, 1);
    history
        .execute(Command::Rename {
            id,
            name: "a".into(),
        })
        .unwrap();
    assert_eq!(history.close_chunk().unwrap(), EditOutcome::Unchanged);
    assert_eq!(history.undo_len(), 1);
}

#[test]
fn a_chunk_edit_clears_redo_once_closed_and_failed_edits_inside_change_nothing() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    let gone = create(&mut history, "gone");
    history.execute(Command::Remove(gone)).unwrap();
    rename(&mut history, id, 1);
    history.undo();
    assert!(history.can_redo());
    history.open_chunk("c").unwrap();
    assert!(history.execute(Command::Remove(gone)).is_err());
    assert_eq!(history.close_chunk().unwrap(), EditOutcome::Unchanged);
    assert!(history.can_redo(), "a failed or empty chunk keeps redo");
    history.open_chunk("c").unwrap();
    create(&mut history, "b");
    history.close_chunk().unwrap();
    assert!(!history.can_redo());
}

#[test]
fn chunk_misuse_is_a_typed_error_and_never_blocks_undo_for_good() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    assert!(matches!(
        history.close_chunk(),
        Err(NativeError::UndoState(_))
    ));
    history.open_chunk("one").unwrap();
    assert!(matches!(
        history.open_chunk("two"),
        Err(NativeError::UndoState(_))
    ));
    assert!(history.set_undo_queue(UndoQueue::Infinite).is_err());
    assert!(history.set_undo_enabled(false, true).is_err());
    rename(&mut history, id, 1);
    assert!(history.undo(), "Undo closes the open chunk and reverts it");
    assert_eq!(history.open_chunk_name(), None);
    assert_eq!(history.scene().nodes()[0].name(), "a");
}

#[test]
fn lowering_the_size_while_redo_holds_entries_never_exceeds_it_through_redo() {
    let mut history = History::default();
    let id = create(&mut history, "a");
    for n in 0..4 {
        rename(&mut history, id, n);
    }
    for _ in 0..4 {
        history.undo();
    }
    history.set_undo_queue(UndoQueue::Finite(2)).unwrap();
    while history.redo() {}
    assert!(history.undo_len() <= 2);
}

#[test]
fn undo_settings_carry_to_a_replacement_history() {
    let mut old = History::default();
    old.set_undo_queue(UndoQueue::Infinite).unwrap();
    old.set_undo_enabled(false, true).unwrap();
    let mut fresh = History::default();
    fresh.inherit_undo_settings(&old);
    assert_eq!(fresh.undo_queue(), UndoQueue::Infinite);
    assert!(!fresh.undo_enabled());
}
