//! Pieces shared by the export handlers: the path text checks and, for glTF and USD, the displayed
//! instances (all, or those under the selected objects).

#[cfg(any(test, feature = "mcp"))]
use crate::app::Editor;
use crate::editor_operations::OperationError;
#[cfg(any(test, feature = "mcp"))]
use azteccraft::native_scene::NativeId;
#[cfg(any(test, feature = "mcp"))]
use azteccraft::view_scene::{ViewInstance, ViewScene};
use std::path::Path;

#[cfg(any(test, feature = "mcp"))]
const VIEW_BUDGET: usize = 256 * 1024 * 1024;

/// The extension-independent path rules, run before any native draft is committed so an invalid
/// path never commits one: UTF-8, 1 to 4096 bytes, no NUL. `noun` leads each message.
pub(in crate::app) fn validate_path_text(path: &Path, noun: &str) -> Result<(), OperationError> {
    let text = path
        .to_str()
        .ok_or_else(|| OperationError::InvalidInput(format!("{noun} path must be UTF-8")))?;
    if text.is_empty() || text.len() > 4096 || text.contains('\0') {
        return Err(OperationError::InvalidInput(format!(
            "{noun} path must contain 1 to 4096 bytes and no NUL"
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "mcp"))]
pub(in crate::app) struct ExportView {
    view: ViewScene,
    selected: Option<Vec<Vec<NativeId>>>,
}

#[cfg(any(test, feature = "mcp"))]
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

#[cfg(any(test, feature = "mcp"))]
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
