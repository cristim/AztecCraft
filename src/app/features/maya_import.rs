//! File > Import of a Maya `.ma` or `.mb` file, shared with MCP `import_maya_scene`. The file is
//! loaded and planned before the scene is touched; the history command then adds it as one undo
//! step.

use crate::app::features::obj_import::ImportTotals;
use crate::app::{Editor, features::export_common::validate_path_text};
use crate::editor_operations::{MayaImportFileError, OperationError};
use azteccraft::{
    document::{self, NativeLoadError},
    history::Command,
    native_scene::MayaImportOptions,
};
use std::path::Path;

/// Path checks shared by the pre-draft guard and the handler, so an invalid path never commits
/// a native draft.
pub(in crate::app) fn validate_maya_import_path(path: &Path) -> Result<(), OperationError> {
    validate_path_text(path, "Import")?;
    let supported = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("ma") || extension.eq_ignore_ascii_case("mb")
    });
    if !supported {
        return Err(OperationError::InvalidInput(
            "Import path must end in .ma or .mb".into(),
        ));
    }
    Ok(())
}

fn file_error(error: &NativeLoadError) -> MayaImportFileError {
    match error {
        NativeLoadError::Read(_) => MayaImportFileError::Io,
        NativeLoadError::TooLarge => MayaImportFileError::TooLarge,
        _ => MayaImportFileError::Invalid,
    }
}

impl Editor {
    /// Loads `path` and plans its meshes against the committed scene. Returns the history
    /// command and what the reply reports; nothing changes on failure.
    pub(in crate::app) fn plan_maya_file_import(
        &self,
        path: &Path,
        prefix: Option<String>,
    ) -> Result<(Command, ImportTotals), OperationError> {
        let source = document::load_native(path)
            .map_err(|error| OperationError::MayaImportFile(file_error(&error)))?;
        let plan = self
            .history
            .scene()
            .plan_maya_import(&source.scene, &MayaImportOptions { prefix })
            .map_err(OperationError::Scene)?;
        let totals = ImportTotals {
            vertices: plan.vertex_count(),
            faces: plan.face_count(),
            ignored_records: 0,
        };
        Ok((Command::ImportMaya(Box::new(plan)), totals))
    }
}
