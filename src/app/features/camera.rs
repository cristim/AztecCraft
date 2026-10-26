use crate::app::*;

impl Editor {
    pub(in crate::app) fn frame_selected(&mut self) {
        self.native_operation(Operation::FrameSelected);
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
