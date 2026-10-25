//! Reply points of an owed OS termination (#261): every way it can end answers the bridge once.
use super::*;
use crate::app::terminate_bridge::TerminateBridge;
use azteccraft::scene::Primitive;
use std::{cell::Cell, cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
struct Probe {
    owed: Rc<Cell<bool>>,
    replies: Rc<RefCell<Vec<bool>>>,
}
impl Probe {
    fn replies(&self) -> Vec<bool> {
        self.replies.borrow().clone()
    }
}
struct FakeBridge(Probe);
impl TerminateBridge for FakeBridge {
    fn owed(&self) -> bool {
        self.0.owed.get()
    }
    fn finish(&mut self, approved: bool) {
        // Like the crate: an answer clears the owed state at once.
        self.0.owed.set(false);
        self.0.replies.borrow_mut().push(approved);
    }
}

struct Rig {
    editor: Editor,
    ctx: egui::Context,
    probe: Probe,
    closes: usize,
    cancels: usize,
}
impl Rig {
    fn new(dirty: bool) -> Self {
        let mut editor = Editor::default();
        if dirty {
            editor.create(Primitive::Cube);
        }
        Self::with(editor)
    }
    fn with(mut editor: Editor) -> Self {
        let probe = Probe::default();
        probe.owed.set(true);
        editor.terminate = Box::new(FakeBridge(probe.clone()));
        Self {
            editor,
            ctx: egui::Context::default(),
            probe,
            closes: 0,
            cancels: 0,
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.run(events, false);
    }
    /// A frame carrying the window's close request, as AppKit's `performClose:` delivers it.
    fn close_requested_frame(&mut self) {
        self.run(vec![], true);
    }
    fn run(&mut self, events: Vec<egui::Event>, close_requested: bool) {
        let mut input = egui::RawInput {
            events,
            ..Default::default()
        };
        if close_requested {
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
        }
        let mut output = self.ctx.run_ui(input, |ui| self.editor.render(ui));
        output.textures_delta.clear();
        let root = &output.viewport_output[&egui::ViewportId::ROOT];
        self.closes += root
            .commands
            .iter()
            .filter(|c| matches!(c, egui::ViewportCommand::Close))
            .count();
        self.cancels += root
            .commands
            .iter()
            .filter(|c| matches!(c, egui::ViewportCommand::CancelClose))
            .count();
    }
    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(vec![]);
        }
    }
    fn escape(&mut self) {
        self.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
    fn pending(&self) -> Uuid {
        self.editor.files.pending.as_ref().unwrap().id
    }
    fn resolve(&mut self, decision: FileDecision) -> Result<OperationReply, OperationError> {
        let pending_id = self.pending();
        self.editor.apply_operation(Operation::ResolveFileAction {
            pending_id,
            decision,
        })
    }
}

#[test]
fn owed_without_a_close_event_raises_the_modal() {
    let mut rig = Rig::new(true);
    rig.frame(vec![]);
    assert!(rig.editor.files.is_modal());
    assert!(rig.probe.replies().is_empty());
    assert_eq!(rig.closes, 0);
}

#[test]
fn owed_clean_scene_answers_true_once_and_never_sends_close() {
    let mut rig = Rig::new(false);
    rig.frames(6);
    assert_eq!(rig.probe.replies(), [true]);
    assert_eq!(rig.closes, 0);
}

#[test]
fn owed_discard_answers_true_once_and_never_sends_close() {
    let mut rig = Rig::new(true);
    rig.frame(vec![]);
    rig.resolve(FileDecision::Discard).unwrap();
    rig.frames(6);
    assert_eq!(rig.probe.replies(), [true]);
    assert_eq!(rig.closes, 0);
}

#[test]
fn not_owed_still_closes_through_the_viewport() {
    let mut rig = Rig::new(false);
    rig.probe.owed.set(false);
    rig.editor.request_close();
    rig.frames(3);
    assert_eq!(rig.closes, 1);
    assert!(rig.probe.replies().is_empty());
}

#[test]
fn owed_cancel_and_escape_answer_false_once_and_a_new_terminate_prompts_again() {
    for escape in [false, true] {
        let mut rig = Rig::new(true);
        rig.frame(vec![]);
        if escape {
            rig.escape();
        } else {
            rig.resolve(FileDecision::Cancel).unwrap();
        }
        rig.frames(4);
        assert_eq!(rig.probe.replies(), [false]);
        assert!(!rig.editor.files.is_closing());
        assert!(!rig.editor.files.is_modal());
        assert_eq!(rig.editor.history.scene().nodes().len(), 1);
        rig.probe.owed.set(true);
        rig.frames(2);
        assert!(rig.editor.files.is_modal());
        assert_eq!(rig.probe.replies(), [false]);
        assert_eq!(rig.closes, 0);
    }
}

#[test]
fn a_pending_input_defers_the_prompt_instead_of_dropping_it() {
    let mut rig = Rig::new(true);
    // Warm up with no termination owed: a first frame can run two passes, clearing the flag.
    rig.probe.owed.set(false);
    rig.frames(2);
    rig.probe.owed.set(true);
    // The frame clears the flag, so only the first frame sees the pending input.
    rig.editor.native_input_pending = true;
    rig.frame(vec![]);
    assert!(!rig.editor.files.is_modal());
    rig.frame(vec![]);
    assert!(rig.editor.files.is_modal());
}

#[test]
fn a_pending_open_is_resolved_first_then_the_owed_close_prompts_itself() {
    let directory = tempfile::tempdir().unwrap();
    let candidate = directory.path().join("candidate.aztec");
    let mut scene = azteccraft::scene::Scene::default();
    scene.create(Primitive::Cube, "candidate").unwrap();
    document::save_new(&candidate, &scene).unwrap();
    let mut rig = Rig::new(true);
    rig.editor
        .apply_operation(Operation::OpenScene { path: candidate })
        .unwrap();
    assert!(matches!(
        rig.editor.files.pending.as_ref().unwrap().action,
        FileAction::Open { .. }
    ));
    rig.frames(3);
    assert!(rig.probe.replies().is_empty());
    rig.resolve(FileDecision::Cancel).unwrap();
    assert!(rig.probe.replies().is_empty());
    rig.frame(vec![]);
    assert!(matches!(
        rig.editor.files.pending.as_ref().unwrap().action,
        FileAction::Close
    ));
    assert!(rig.probe.replies().is_empty());
    rig.resolve(FileDecision::Cancel).unwrap();
    assert_eq!(rig.probe.replies(), [false]);
    assert_eq!(rig.closes, 0);
}

#[test]
fn a_late_close_request_is_cancelled_while_owed_and_after_the_approving_reply() {
    let mut rig = Rig::new(false);
    rig.frame(vec![]);
    assert!(rig.editor.files.is_closing());
    // Owed and approved: the approving reply is due this very frame.
    rig.close_requested_frame();
    assert_eq!(rig.probe.replies(), [true]);
    assert_eq!(rig.cancels, 1);
    // After the reply (no longer owed) the request is still not allowed to close the viewport.
    rig.close_requested_frame();
    assert_eq!(rig.cancels, 2);
    assert_eq!(rig.closes, 0);
}

#[test]
fn a_viewport_close_already_sent_is_refused_instead_of_exiting_twice() {
    let mut rig = Rig::new(false);
    rig.probe.owed.set(false);
    rig.editor.request_close();
    rig.frames(2);
    assert_eq!(rig.closes, 1);
    rig.probe.owed.set(true);
    rig.frames(3);
    assert_eq!(rig.probe.replies(), [false]);
    assert_eq!(rig.closes, 1);
}

#[test]
fn a_refused_close_request_answers_false_instead_of_retrying_forever() {
    let mut rig = Rig::new(true);
    rig.editor.name = String::new();
    rig.frames(4);
    assert_eq!(rig.probe.replies(), [false]);
    assert!(rig.editor.error);
    assert!(!rig.editor.files.is_modal());
    assert_eq!(rig.closes, 0);
}

#[test]
fn owed_save_of_an_untitled_scene_over_the_remote_path_answers_false() {
    let mut rig = Rig::new(true);
    rig.frame(vec![]);
    assert!(rig.resolve(FileDecision::Save).is_err());
    assert_eq!(rig.probe.replies(), [false]);
    rig.frames(3);
    assert_eq!(rig.probe.replies(), [false]);
    assert!(rig.editor.is_dirty());
    assert_eq!(rig.closes, 0);
}

#[cfg(unix)]
mod saving {
    use super::*;
    use crate::app::files::binary_fixture;
    use crate::app::files::tests::relocate_completed_stage_once;

    fn edited_titled(dir: &Path) -> Rig {
        let source = dir.join("scene.mb");
        std::fs::write(&source, binary_fixture::specimen()).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene { path: source })
            .unwrap();
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        Rig::with(editor)
    }

    #[test]
    fn owed_save_success_answers_true_once_and_never_sends_close() {
        let dir = tempfile::tempdir().unwrap();
        let mut rig = edited_titled(dir.path());
        rig.frame(vec![]);
        rig.resolve(FileDecision::Save).unwrap();
        assert!(!rig.editor.is_dirty());
        rig.frames(6);
        assert_eq!(rig.probe.replies(), [true]);
        assert_eq!(rig.closes, 0);
    }

    #[test]
    fn owed_save_failure_answers_false_and_keeps_the_scene() {
        let dir = tempfile::tempdir().unwrap();
        let mut rig = edited_titled(dir.path());
        rig.frame(vec![]);
        std::fs::write(dir.path().join("scene.mb"), b"changed behind the editor").unwrap();
        assert!(rig.resolve(FileDecision::Save).is_err());
        assert_eq!(rig.probe.replies(), [false]);
        rig.frames(4);
        assert_eq!(rig.probe.replies(), [false]);
        assert!(rig.editor.is_dirty());
        assert!(!rig.editor.files.is_closing());
        assert_eq!(rig.closes, 0);
    }

    #[test]
    fn owed_uncertain_save_answers_false_and_stays_reconcilable() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("document");
        let moved = dir.path().join("moved");
        std::fs::create_dir(&parent).unwrap();
        let mut rig = edited_titled(&parent);
        let expected =
            azteccraft::native_scene::binary::encode(rig.editor.history.scene()).unwrap();
        rig.frame(vec![]);
        let relocated = Cell::new(false);
        let guard = || relocate_completed_stage_once(&parent, &moved, &expected, &relocated);
        let pending_id = rig.pending();
        let reply = rig
            .editor
            .apply_operation_guarded(
                azteccraft::history::ManipulationProvenance::Native,
                Operation::ResolveFileAction {
                    pending_id,
                    decision: FileDecision::Save,
                },
                &guard,
            )
            .unwrap();
        assert!(matches!(reply, OperationReply::SaveUncertain { .. }));
        assert_eq!(rig.probe.replies(), [false]);
        rig.frames(4);
        assert_eq!(rig.probe.replies(), [false]);
        assert!(rig.editor.files.reconciliation_required());
        assert!(rig.editor.files.is_modal());
        assert_eq!(rig.closes, 0);
        std::fs::rename(&moved, &parent).unwrap();
    }
}

#[cfg(feature = "mcp")]
mod mcp_delivery {
    use super::*;
    use crate::mcp::CloseDelivery;

    fn approved_rig() -> (Rig, crate::mcp::TestIngress) {
        let mut rig = Rig::new(false);
        let (bridge, ingress) = crate::mcp::test_bridge(rig.ctx.clone());
        rig.editor.mcp = Some(bridge);
        rig.frame(vec![]);
        assert!(rig.editor.files.is_closing());
        (rig, ingress)
    }

    #[test]
    fn waiting_holds_the_answer_until_the_response_is_flushed() {
        let (mut rig, ingress) = approved_rig();
        ingress.force_close_delivery(CloseDelivery::Waiting);
        rig.frames(3);
        assert!(rig.probe.replies().is_empty());
        assert!(rig.probe.owed.get());
        ingress.force_close_delivery(CloseDelivery::Flushed);
        rig.frames(3);
        assert_eq!(rig.probe.replies(), [true]);
        assert_eq!(rig.closes, 0);
    }

    #[test]
    fn failed_delivery_answers_false_once_without_a_stale_approval_or_new_prompt() {
        let (mut rig, ingress) = approved_rig();
        ingress.force_close_delivery(CloseDelivery::Failed);
        rig.frame(vec![]);
        assert_eq!(rig.probe.replies(), [false]);
        assert!(!rig.editor.files.is_closing());
        assert!(rig.editor.error);
        rig.frames(3);
        assert_eq!(rig.probe.replies(), [false]);
        assert_eq!(rig.closes, 0);
    }
}
