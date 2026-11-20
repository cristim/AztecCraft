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
            | Operation::SetView(_)
            | Operation::FrameSelected { .. }
            | Operation::FrameAll { .. }
            | Operation::ApplyCameraBookmark { .. }
            | Operation::DefaultView
    )
}
