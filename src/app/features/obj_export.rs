//! File > Export All / Export Selection as Wavefront OBJ, shared with MCP `export_obj`.

use crate::app::Editor;
use crate::editor_operations::{ObjExportError, Operation, OperationError, OperationReply};
use azteccraft::obj::{self, ObjError};
use azteccraft::view_scene::{ViewInstance, ViewScene};
use std::path::{Path, PathBuf};

const VIEW_BUDGET: usize = 256 * 1024 * 1024;

/// Path checks shared by the pre-draft guard and the handler, so an invalid path never commits
/// a native draft.
pub(in crate::app) fn validate_export_path(path: &Path) -> Result<(), OperationError> {
    let text = path
        .to_str()
        .ok_or_else(|| OperationError::InvalidInput("Export path must be UTF-8".into()))?;
    if text.is_empty() || text.len() > 4096 || text.contains('\0') {
        return Err(OperationError::InvalidInput(
            "Export path must contain 1 to 4096 bytes and no NUL".into(),
        ));
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
    {
        return Err(OperationError::InvalidInput(
            "Export path must end in .obj".into(),
        ));
    }
    Ok(())
}

impl Editor {
    /// Writes displayed polygon geometry to a new `.obj` file: every instance, or the instances
    /// under the selected object. Reads the committed scene and changes nothing in it.
    pub(in crate::app) fn export_obj(
        &mut self,
        path: &Path,
        selection_only: bool,
    ) -> Result<OperationReply, OperationError> {
        validate_export_path(path)?;
        if self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        let selected = if selection_only {
            let (id, path) = self
                .single_target()
                .map_err(|_| OperationError::NoSelection)?;
            let path = path.to_vec();
            Some(self.selection_path(id, (!path.is_empty()).then_some(path))?)
        } else {
            None
        };
        let scene = self.history.scene();
        let view = ViewScene::from_native(scene, VIEW_BUDGET)
            .map_err(|error| OperationError::View(error.to_string()))?;
        let instances: Vec<&ViewInstance> = view
            .instances()
            .iter()
            .filter(|instance| {
                selected
                    .as_ref()
                    .is_none_or(|path| instance.path().starts_with(path))
            })
            .collect();
        let summary = obj::export_native_new(path, scene, &instances).map_err(|error| {
            OperationError::ObjExport(match error {
                ObjError::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::AlreadyExists =>
                {
                    ObjExportError::DestinationExists
                }
                ObjError::Io { source, .. } => ObjExportError::Io(source.kind()),
                ObjError::NothingToExport => ObjExportError::NothingToExport,
                error => ObjExportError::Invalid(error.to_string()),
            })
        })?;
        Ok(OperationReply::Exported {
            objects: summary.objects,
            vertices: summary.vertices,
            faces: summary.faces,
        })
    }

    pub(in crate::app) fn export_dialog(&mut self, selection_only: bool) {
        if self.files.is_modal() {
            return;
        }
        let title = if selection_only {
            "Export Selection as OBJ to a new file"
        } else {
            "Export All as OBJ to a new file"
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_title(title)
            .add_filter("OBJ", &["obj"])
            .save_file()
        {
            self.run_export(path, selection_only);
        }
    }

    pub(in crate::app) fn run_export(&mut self, path: PathBuf, selection_only: bool) {
        self.native_operation(Operation::ExportObj {
            path,
            selection_only,
        });
    }
}
