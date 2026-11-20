use crate::app::*;

impl Editor {
    pub(in crate::app) fn frame_selected(&mut self, with_children: bool) {
        self.native_operation(Operation::FrameSelected {
            with_children,
            fit_factor: None,
        });
    }

    /// Frames the selected objects. With `Exclude`, a selection none of whose members owns visible
    /// geometry frames the members' origins instead (PROVISIONAL: Maya's docs are silent; the
    /// manipulator origin is the rotate pivot, not the matrix translation).
    pub(in crate::app) fn frame_members(
        &mut self,
        children: FrameChildren,
    ) -> Result<(), OperationError> {
        let paths = self.selected_member_paths()?;
        let view = ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024)
            .map_err(|error| OperationError::View(error.to_string()))?;
        let prefixes: Vec<&[NativeId]> = paths.iter().map(Vec::as_slice).collect();
        if children == FrameChildren::Exclude && !view.has_direct_geometry(&prefixes) {
            let mut origins = Vec::with_capacity(paths.len());
            for (member, path) in self.selection.members().iter().zip(&paths) {
                let target = TransformTarget {
                    node: member.node,
                    path: path.clone(),
                };
                let frame = self
                    .history
                    .scene()
                    .manipulator_frame(&target)
                    .map_err(OperationError::Scene)?;
                // T + RT on purpose: framing ignores the rotate pivot (see `ManipulatorFrame::origin_cm`).
                origins.push(frame.origin_cm.as_vec3());
            }
            return self
                .camera
                .frame_points(&origins, self.aspect)
                .map_err(OperationError::Camera);
        }
        self.camera
            .frame_view_many(&view, &prefixes, children, self.aspect)
            .map_err(OperationError::Camera)
    }

    /// The panel name: `persp`, `top`, `front`, `side`, or `ortho` for a hand-posed orthographic camera.
    pub(in crate::app) fn view_name(&self) -> &'static str {
        match (self.camera.projection(), self.camera.orthographic_view()) {
            (Projection::Perspective, _) => "persp",
            (_, Some(OrthographicView::Top)) => "top",
            (_, Some(OrthographicView::Front)) => "front",
            (_, Some(OrthographicView::Side)) => "side",
            (_, None) => "ortho",
        }
    }

    pub(in crate::app) fn show_view(
        &mut self,
        view: OrthographicView,
    ) -> Result<(), OperationError> {
        self.camera
            .show_orthographic(view)
            .map_err(OperationError::Camera)
    }

    pub(in crate::app) fn default_view(&mut self) {
        self.native_operation(Operation::DefaultView);
    }

    /// Whether F, Ctrl+F and their menu rows can act: a selection, or Object and Hierarchy modes,
    /// where an empty selection frames everything.
    pub(in crate::app) fn can_frame_selected(&self) -> bool {
        !self.selection.is_empty()
            || matches!(
                self.selection_mode,
                SelectionMode::Object | SelectionMode::Hierarchy
            )
    }

    pub(in crate::app) fn frame_everything(&mut self) -> Result<OperationReply, OperationError> {
        let view = ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024)
            .map_err(|error| OperationError::View(error.to_string()))?;
        if view.instances().is_empty()
            && view.curve_instances().is_empty()
            && view.joints().is_empty()
        {
            return Err(OperationError::NothingToFrame);
        }
        let before = self.camera.clone();
        self.camera
            .frame_view(&view, None, self.aspect)
            .map_err(OperationError::Camera)?;
        Ok(if self.camera == before {
            OperationReply::Unchanged
        } else {
            OperationReply::Changed
        })
    }

    /// Starts a viewFit Fit Factor framing: with a factor, swaps in a fitted copy of the camera and
    /// returns the original; an out of range factor is refused before any change.
    pub(in crate::app) fn begin_fit(
        &mut self,
        fit_factor: Option<f32>,
    ) -> Result<Option<Camera>, OperationError> {
        let Some(fit_factor) = fit_factor else {
            return Ok(None);
        };
        let fitted = self.camera.fitted(fit_factor).map_err(|_| {
            OperationError::InvalidInput("fit_factor must be a number from 0.01 to 1".into())
        })?;
        Ok(Some(std::mem::replace(&mut self.camera, fitted)))
    }

    /// Ends `begin_fit`: a successful framing keeps its pose with the default margin again; a
    /// failed one restores the original camera.
    pub(in crate::app) fn end_fit<T>(
        &mut self,
        original: Option<Camera>,
        result: Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        if let Some(original) = original {
            self.camera = if result.is_ok() {
                self.camera.clone().unfitted()
            } else {
                original
            };
        }
        result
    }

    pub(in crate::app) fn frame_all(&mut self) {
        self.native_operation(Operation::FrameAll { fit_factor: None });
    }
}

/// Session viewport history, separate from scene authoring Undo.
#[derive(Default)]
pub(in crate::app) struct ViewHistory {
    pub undo: std::collections::VecDeque<Camera>,
    pub redo: std::collections::VecDeque<Camera>,
    grouped: bool,
}

impl ViewHistory {
    // ponytail: fixed 50 session views; add a preference only when its UI is implemented.
    const LIMIT: usize = 50;

    pub fn end_gesture(&mut self) {
        self.grouped = false;
    }

    pub fn record(&mut self, before: Camera, grouped: bool) {
        if !grouped || !self.grouped {
            if self.undo.len() == Self::LIMIT {
                self.undo.pop_front();
            }
            self.undo.push_back(before);
        }
        self.redo.clear();
        self.grouped = grouped;
    }

    pub fn step(&mut self, camera: &mut Camera, redo: bool) -> OperationReply {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some(previous) = from.pop_back() else {
            return OperationReply::Unchanged;
        };
        if to.len() == Self::LIMIT {
            to.pop_front();
        }
        to.push_back(std::mem::replace(camera, previous));
        self.end_gesture();
        OperationReply::Changed
    }
}

pub(in crate::app) fn changes_camera(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::Orbit { .. }
            | Operation::Pan { .. }
            | Operation::Dolly { .. }
            | Operation::BoxDolly { .. }
            | Operation::SetCameraPose { .. }
            | Operation::SetCameraFilmBack(_)
            | Operation::SetView(_)
            | Operation::FrameSelected { .. }
            | Operation::FrameAll { .. }
            | Operation::ApplyCameraBookmark { .. }
            | Operation::DefaultView
    )
}

#[cfg(test)]
mod view_history_tests {
    use super::*;
    use crate::app::tests::remote;

    #[test]
    fn full_cameras_round_trip_independently_of_scene_undo() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let scene = editor.history.scene().clone();
        let dirty = editor.is_dirty();
        let token = editor.scene_token;
        let mut views = vec![editor.camera.clone()];
        for op in [
            Operation::Orbit {
                radians: [0.2, 0.1],
            },
            Operation::Pan {
                world_units: [1.0, 2.0],
            },
            Operation::Dolly { factor: 0.5 },
            Operation::SetView(ViewPreset::Top),
            Operation::DefaultView,
        ] {
            remote(&mut editor, op).unwrap();
            views.push(editor.camera.clone());
        }
        for expected in views.iter().rev().skip(1) {
            assert!(matches!(
                remote(&mut editor, Operation::UndoView).unwrap(),
                OperationReply::Changed
            ));
            assert_eq!(&editor.camera, expected);
        }
        assert!(matches!(
            remote(&mut editor, Operation::UndoView).unwrap(),
            OperationReply::Unchanged
        ));
        for expected in views.iter().skip(1) {
            remote(&mut editor, Operation::RedoView).unwrap();
            assert_eq!(&editor.camera, expected);
        }
        assert!(matches!(
            remote(&mut editor, Operation::RedoView).unwrap(),
            OperationReply::Unchanged
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.is_dirty(), dirty);
        assert!(editor.history.can_undo());
    }

    #[test]
    fn failed_and_unchanged_navigation_preserve_forward_views_and_replacement_resets() {
        let mut editor = Editor::default();
        remote(&mut editor, Operation::Dolly { factor: 0.5 }).unwrap();
        remote(&mut editor, Operation::UndoView).unwrap();
        let before = editor.camera.clone();
        remote(&mut editor, Operation::Dolly { factor: 1.0 }).unwrap();
        assert!(remote(&mut editor, Operation::Dolly { factor: 0.0 }).is_err());
        assert_eq!(editor.camera, before);
        assert_eq!(editor.view_history.redo.len(), 1);
        let dir = tempfile::tempdir().unwrap();
        assert!(
            editor
                .apply_operation(Operation::OpenScene {
                    path: dir.path().join("missing.ma")
                })
                .is_err()
        );
        assert_eq!(editor.view_history.redo.len(), 1);
        remote(
            &mut editor,
            Operation::Pan {
                world_units: [1.0, 0.0],
            },
        )
        .unwrap();
        assert!(editor.view_history.redo.is_empty());
        editor.apply_operation(Operation::NewScene).unwrap();
        assert!(editor.view_history.undo.is_empty());
        assert!(editor.view_history.redo.is_empty());
    }

    #[test]
    fn native_drag_groups_and_discrete_navigation_breaks_the_group_and_history_is_bounded() {
        let mut editor = Editor::default();
        let initial = editor.camera.clone();
        editor.native_camera_drag = true;
        for _ in 0..3 {
            editor
                .apply_operation(Operation::Orbit {
                    radians: [0.1, 0.0],
                })
                .unwrap();
        }
        assert_eq!(editor.view_history.undo.len(), 1);
        editor.apply_operation(Operation::DefaultView).unwrap();
        editor
            .apply_operation(Operation::Orbit {
                radians: [0.1, 0.0],
            })
            .unwrap();
        assert_eq!(editor.view_history.undo.len(), 3);
        editor.native_camera_drag = false;
        editor.apply_operation(Operation::UndoView).unwrap();
        editor.apply_operation(Operation::UndoView).unwrap();
        editor.apply_operation(Operation::UndoView).unwrap();
        assert_eq!(editor.camera, initial);
        for _ in 0..70 {
            remote(
                &mut editor,
                Operation::Pan {
                    world_units: [0.1, 0.0],
                },
            )
            .unwrap();
        }
        assert_eq!(ViewHistory::LIMIT, 50, "development.md row 59");
        assert_eq!(editor.view_history.undo.len(), ViewHistory::LIMIT);
        for _ in 0..70 {
            remote(&mut editor, Operation::UndoView).unwrap();
        }
        assert!(editor.view_history.undo.is_empty());
        assert_eq!(editor.view_history.redo.len(), ViewHistory::LIMIT);
    }

    #[test]
    fn native_pointer_drags_group_but_a_coalesced_release_and_press_starts_a_new_view() {
        use crate::app::tests::frame;
        let mut editor = Editor::default();
        let ctx = egui::Context::default();
        for _ in 0..4 {
            frame(&mut editor, &ctx, vec![]);
        }
        let initial = editor.camera.clone();
        let alt = egui::Modifiers::ALT;
        let p = egui::pos2(600.0, 400.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: alt,
        };
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(alt),
                egui::Event::PointerMoved(p),
                button(p, true),
            ],
        );
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(p + egui::vec2(20.0, 10.0))],
        );
        let end = p + egui::vec2(40.0, 20.0);
        frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(end)]);
        let first = editor.camera.clone();
        assert_ne!(first, initial);
        assert_eq!(editor.view_history.undo.len(), 1);
        frame(
            &mut editor,
            &ctx,
            vec![button(end, false), button(end, true)],
        );
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::PointerMoved(end + egui::vec2(20.0, 10.0))],
        );
        assert_eq!(editor.view_history.undo.len(), 2);
        frame(
            &mut editor,
            &ctx,
            vec![
                button(end, false),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        editor.apply_operation(Operation::UndoView).unwrap();
        assert_eq!(editor.camera, first);
        editor.apply_operation(Operation::UndoView).unwrap();
        assert_eq!(editor.camera, initial);
    }

    #[test]
    fn bracket_keys_dispatch_view_history() {
        let mut editor = Editor::default();
        let initial = editor.camera.clone();
        editor
            .apply_operation(Operation::Dolly { factor: 0.5 })
            .unwrap();
        let changed = editor.camera.clone();
        let ctx = egui::Context::default();
        for (key, expected) in [(Key::OpenBracket, initial), (Key::CloseBracket, changed)] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| {
                    editor.shortcuts(ui.ctx());
                },
            );
            output.textures_delta.clear();
            assert_eq!(editor.camera, expected);
        }
    }
}
