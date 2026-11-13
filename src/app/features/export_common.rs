//! Pieces shared by the glTF and USD export handlers: the path text checks and the displayed
//! instances (all, or those under the selected objects).

use crate::app::Editor;
use crate::editor_operations::OperationError;
use azteccraft::native_scene::NativeId;
use azteccraft::view_scene::{ViewInstance, ViewScene};
use std::path::Path;

const VIEW_BUDGET: usize = 256 * 1024 * 1024;

/// Path checks run before any native draft is committed, so an invalid path never commits one.
pub(in crate::app) fn validate_path_text(path: &Path) -> Result<(), OperationError> {
    let text = path
        .to_str()
        .ok_or_else(|| OperationError::InvalidInput("Export path must be UTF-8".into()))?;
    if text.is_empty() || text.len() > 4096 || text.contains('\0') {
        return Err(OperationError::InvalidInput(
            "Export path must contain 1 to 4096 bytes and no NUL".into(),
        ));
    }
    Ok(())
}

pub(in crate::app) struct ExportView {
    view: ViewScene,
    selected: Option<Vec<Vec<NativeId>>>,
}

impl ExportView {
    pub(in crate::app) fn instances(&self) -> Vec<&ViewInstance> {
        self.view
            .instances()
            .iter()
            .filter(|instance| {
                self.selected
                    .as_ref()
                    .is_none_or(|paths| paths.iter().any(|path| instance.path().starts_with(path)))
            })
            .collect()
    }
}

impl Editor {
    /// Refuses while a modal is open, then derives the view of the committed scene.
    pub(in crate::app) fn export_view(
        &self,
        selection_only: bool,
    ) -> Result<ExportView, OperationError> {
        if self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        let selected = selection_only
            .then(|| self.selected_member_paths())
            .transpose()?;
        let view = ViewScene::from_native(self.history.scene(), VIEW_BUDGET)
            .map_err(|error| OperationError::View(error.to_string()))?;
        Ok(ExportView { view, selected })
    }
}
