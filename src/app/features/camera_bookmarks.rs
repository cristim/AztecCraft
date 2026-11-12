use crate::app::*;
use azteccraft::camera_bookmarks::BookmarkPose;

impl Editor {
    pub(in crate::app) fn create_camera_bookmark(
        &mut self,
        name: &str,
    ) -> Result<OperationReply, OperationError> {
        self.bookmarks
            .create(name, BookmarkPose::of(&self.camera))
            .map_err(OperationError::CameraBookmark)?;
        Ok(OperationReply::Changed)
    }

    pub(in crate::app) fn rename_camera_bookmark(
        &mut self,
        name: &str,
        new_name: &str,
    ) -> Result<OperationReply, OperationError> {
        let changed = self
            .bookmarks
            .rename(name, new_name)
            .map_err(OperationError::CameraBookmark)?;
        Ok(if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        })
    }

    pub(in crate::app) fn delete_camera_bookmark(
        &mut self,
        name: &str,
    ) -> Result<OperationReply, OperationError> {
        self.bookmarks
            .delete(name)
            .map_err(OperationError::CameraBookmark)?;
        Ok(OperationReply::Changed)
    }

    pub(in crate::app) fn apply_camera_bookmark(
        &mut self,
        name: &str,
    ) -> Result<OperationReply, OperationError> {
        let pose = self
            .bookmarks
            .get(name)
            .map_err(OperationError::CameraBookmark)?
            .pose;
        let before = self.camera.clone();
        pose.apply(&mut self.camera)
            .map_err(OperationError::CameraPose)?;
        Ok(if self.camera == before {
            OperationReply::Unchanged
        } else {
            OperationReply::Changed
        })
    }

    #[cfg(any(test, feature = "mcp"))]
    pub(in crate::app) fn list_camera_bookmarks(&self) -> OperationReply {
        use crate::editor_operations::CameraBookmarkInspection;
        OperationReply::CameraBookmarks {
            bookmarks: self
                .bookmarks
                .list()
                .iter()
                .map(|b| CameraBookmarkInspection {
                    name: b.name.clone(),
                    target: b.pose.target,
                    distance: b.pose.distance,
                    yaw_degrees: f64::from(b.pose.yaw).to_degrees(),
                    pitch_degrees: f64::from(b.pose.pitch).to_degrees(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "camera_bookmarks/tests.rs"]
mod tests;
