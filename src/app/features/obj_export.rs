//! File > Export All / Export Selection as Wavefront OBJ, shared with MCP `export_obj`.

use super::export_common::validate_path_text;
use crate::app::Editor;
use crate::editor_operations::{ObjExportError, Operation, OperationError, OperationReply};
use azteccraft::obj::{self, ObjError};
use azteccraft::view_scene::{VIEW_BUDGET, ViewInstance, ViewScene};
use std::path::{Path, PathBuf};

/// Path checks shared by the pre-draft guard and the handler, so an invalid path never commits
/// a native draft.
pub(in crate::app) fn validate_export_path(path: &Path) -> Result<(), OperationError> {
    validate_obj_path(path, "Export")
}

/// The `.obj` path rules of Export and Import; `noun` leads each message.
pub(in crate::app) fn validate_obj_path(path: &Path, noun: &str) -> Result<(), OperationError> {
    validate_path_text(path, noun)?;
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
    {
        return Err(OperationError::InvalidInput(format!(
            "{noun} path must end in .obj"
        )));
    }
    Ok(())
}

/// The path a file panel result becomes: one trailing dot is dropped, then `.obj` is appended
/// unless the extension already is `obj` (any case). Maya's "Default file extensions" option is
/// on by default; whether each OS panel appends it itself is PROVISIONAL. MCP stays strict.
fn with_default_extension(path: PathBuf) -> PathBuf {
    let trimmed = path
        .to_str()
        .and_then(|text| text.strip_suffix('.'))
        .map(PathBuf::from)
        .unwrap_or(path);
    if trimmed
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"))
    {
        return trimmed;
    }
    let mut text = trimmed.into_os_string();
    text.push(".obj");
    PathBuf::from(text)
}

impl Editor {
    /// Writes displayed polygon geometry to a new `.obj` file: every instance, or the instances
    /// under the selected objects. Reads the committed scene and changes nothing in it.
    pub(in crate::app) fn export_obj(
        &mut self,
        path: &Path,
        selection_only: bool,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        validate_export_path(path)?;
        if self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        let selected = selection_only
            .then(|| self.selected_member_paths())
            .transpose()?;
        let scene = self.history.scene();
        let view = ViewScene::from_native(scene, VIEW_BUDGET)
            .map_err(|error| OperationError::View(error.to_string()))?;
        let instances: Vec<&ViewInstance> = view
            .instances()
            .iter()
            .filter(|instance| {
                selected
                    .as_ref()
                    .is_none_or(|paths| paths.iter().any(|path| instance.path().starts_with(path)))
            })
            .collect();
        let summary =
            obj::export_native_new(path, scene, &instances, may_install).map_err(|error| {
                OperationError::ObjExport(match error {
                    ObjError::Io { source, .. }
                        if source.kind() == std::io::ErrorKind::AlreadyExists =>
                    {
                        ObjExportError::DestinationExists
                    }
                    ObjError::Io { source, .. } => ObjExportError::Io(source.kind()),
                    ObjError::NothingToExport => ObjExportError::NothingToExport,
                    ObjError::Cancelled => ObjExportError::Cancelled,
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
            .set_file_name("untitled.obj")
            .add_filter("OBJ", &["obj"])
            .save_file()
        {
            self.run_export(with_default_extension(path), selection_only);
        }
    }

    pub(in crate::app) fn run_export(&mut self, path: PathBuf, selection_only: bool) {
        self.native_operation(Operation::ExportObj {
            path,
            selection_only,
        });
    }
}

#[cfg(test)]
#[path = "obj_export_tests.rs"]
mod tests;
