use crate::app::*;

impl Editor {
    pub(in crate::app) fn frame_selected(&mut self, with_children: bool) {
        self.native_operation(Operation::FrameSelected { with_children });
    }

    /// Frames the selected objects. With `Exclude`, a selection none of whose members owns visible
    /// geometry frames the members' origins instead (PROVISIONAL: Maya's docs are silent; the
    /// manipulator origin is the rotate pivot, not the matrix translation).
    pub(in crate::app) fn frame_members(
        &mut self,
        children: FrameChildren,
    ) -> Result<(), OperationError> {
        if self.selection.is_empty() {
            return Err(OperationError::NoSelection);
        }
        let mut paths = Vec::with_capacity(self.selection.len());
        for member in self.selection.members() {
            let path = (!member.path.is_empty()).then(|| member.path.clone());
            paths.push(self.selection_path(member.node, path)?);
        }
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

    pub(in crate::app) fn default_view(&mut self) {
        let default = Camera::default();
        self.native_operation(Operation::SetCameraPose {
            target: default.target().to_array(),
            distance: default.distance(),
            yaw: default.yaw(),
            pitch: default.pitch(),
        });
    }

    pub(in crate::app) fn frame_all(&mut self) {
        self.native_operation(Operation::FrameAll);
    }
}
