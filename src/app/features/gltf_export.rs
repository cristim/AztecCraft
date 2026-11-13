//! Export All / Export Selection as glTF 2.0, shared with MCP `export_gltf`. The selection logic
//! mirrors `obj_export.rs` (kept separate while that file is in an open PR).

use crate::app::Editor;
use crate::editor_operations::{GltfExportError, OperationError, OperationReply};
use azteccraft::gltf::{self, GltfError, GltfFormat};
use azteccraft::view_scene::{ViewInstance, ViewScene};
use std::path::Path;

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
    if GltfFormat::from_path(path).is_none() {
        return Err(OperationError::InvalidInput(
            "Export path must end in .gltf or .glb".into(),
        ));
    }
    Ok(())
}

impl Editor {
    /// Writes displayed polygon geometry to a new `.gltf` or `.glb` file: every instance, or the
    /// instances under the selected objects. Reads the committed scene and changes nothing in it.
    pub(in crate::app) fn export_gltf(
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
            gltf::export_native_new(path, scene, &instances, may_install).map_err(|error| {
                OperationError::GltfExport(match error {
                    GltfError::Io { source, .. }
                        if source.kind() == std::io::ErrorKind::AlreadyExists =>
                    {
                        GltfExportError::DestinationExists
                    }
                    GltfError::Io { source, .. } => GltfExportError::Io(source.kind()),
                    GltfError::NothingToExport => GltfExportError::NothingToExport,
                    GltfError::Cancelled => GltfExportError::Cancelled,
                    error @ (GltfError::SessionUvs | GltfError::SessionMaterial) => {
                        GltfExportError::Unsupported(error.to_string())
                    }
                    error => GltfExportError::Invalid(error.to_string()),
                })
            })?;
        Ok(OperationReply::GltfExported {
            objects: summary.objects,
            vertices: summary.vertices,
            triangles: summary.triangles,
        })
    }
}
