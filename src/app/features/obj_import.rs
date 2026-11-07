//! File > Import of a Wavefront OBJ file, shared with MCP `import_obj`. The file is read and
//! planned before the scene is touched; the history command then adds it as one undo step.

use crate::app::{Editor, features::obj_export::validate_obj_path};
use crate::editor_operations::OperationError;
use azteccraft::{history::Command, obj::import};
use std::path::Path;

/// Path checks shared by the pre-draft guard and the handler, so an invalid path never commits
/// a native draft.
pub(in crate::app) fn validate_import_path(path: &Path) -> Result<(), OperationError> {
    validate_obj_path(path, "Import")
}

/// Counts the import reply reports besides the new node ids.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct ImportTotals {
    pub vertices: usize,
    pub faces: usize,
    pub ignored_records: usize,
}

impl Editor {
    /// Reads `path` and plans its objects against the committed scene. Returns the history
    /// command and what the reply reports; nothing changes on failure.
    pub(in crate::app) fn plan_obj_import(
        &self,
        path: &Path,
    ) -> Result<(Command, ImportTotals), OperationError> {
        let parsed = import::read(path).map_err(OperationError::ObjImport)?;
        let plan = self
            .history
            .scene()
            .plan_obj_import(&parsed)
            .map_err(OperationError::Scene)?;
        let totals = ImportTotals {
            vertices: plan.vertex_count(),
            faces: plan.face_count(),
            ignored_records: parsed.ignored_records,
        };
        Ok((Command::ImportObjects(Box::new(plan)), totals))
    }
}
