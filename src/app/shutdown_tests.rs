//! SIGTERM through the shared close guard (#261, PROVISIONAL), driven through the flag the
//! signal thread sets, with real Editor and egui frames.
use super::*;
use azteccraft::scene::Primitive;

struct Rig {
    editor: Editor,
    ctx: egui::Context,
    closes: usize,
}
impl Rig {
    fn new(dirty: bool) -> Self {
        let mut editor = Editor::default();
        if dirty {
            editor.create(Primitive::Cube);
        }
        Self {
            editor,
            ctx: egui::Context::default(),
            closes: 0,
        }
    }
    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            let mut output = self
                .ctx
                .run_ui(egui::RawInput::default(), |ui| self.editor.render(ui));
            output.textures_delta.clear();
            self.closes += output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .filter(|c| matches!(c, egui::ViewportCommand::Close))
                .count();
        }
    }
    fn sigterm(&mut self) {
        self.editor.shutdown.raise_for_test();
    }
    fn cancel(&mut self) {
        let pending_id = self.editor.files.pending.as_ref().unwrap().id;
        self.editor
            .apply_operation(Operation::ResolveFileAction {
                pending_id,
                decision: FileDecision::Cancel,
            })
            .unwrap();
    }
}

#[test]
fn sigterm_on_a_dirty_scene_raises_the_close_prompt_and_does_not_close() {
    let mut rig = Rig::new(true);
    rig.frames(2);
    assert!(!rig.editor.files.is_modal());
    rig.sigterm();
    rig.frames(3);
    assert!(rig.editor.files.is_modal());
    assert!(matches!(
        rig.editor.files.pending.as_ref().map(|p| &p.action),
        Some(FileAction::Close)
    ));
    assert_eq!(rig.closes, 0);
}

#[test]
fn sigterm_on_a_clean_scene_closes_through_the_normal_path() {
    let mut rig = Rig::new(false);
    rig.sigterm();
    rig.frames(3);
    assert_eq!(rig.closes, 1);
}

#[test]
fn cancel_re_arms_the_prompt_instead_of_leaving_the_next_sigterm_fatal() {
    let mut rig = Rig::new(true);
    rig.sigterm();
    rig.frames(3);
    assert!(rig.editor.shutdown.is_armed());
    rig.cancel();
    rig.frames(3);
    assert!(!rig.editor.files.is_modal());
    assert!(
        !rig.editor.shutdown.is_armed(),
        "a second SIGTERM would now kill"
    );
    rig.sigterm();
    rig.frames(3);
    assert!(rig.editor.files.is_modal());
    assert_eq!(rig.closes, 0);
}

#[test]
fn a_sigterm_behind_another_prompt_waits_for_it() {
    let mut rig = Rig::new(true);
    rig.editor.request_new();
    rig.frames(2);
    assert!(rig.editor.files.is_modal());
    rig.sigterm();
    rig.frames(3);
    assert!(matches!(
        rig.editor.files.pending.as_ref().map(|p| &p.action),
        Some(FileAction::New)
    ));
    assert!(rig.editor.shutdown.requested());
}
