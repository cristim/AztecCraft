//! Export All / Export Selection as glTF 2.0, shared with MCP `export_gltf`. The selection logic
//! mirrors `obj_export.rs` (kept separate while that file is in an open PR).

use super::export_common::{ExportView, validate_path_text};
use crate::app::Editor;
use crate::editor_operations::{ExportError, OperationError, OperationReply};
use azteccraft::gltf::{self, GltfError, GltfFormat};
use std::path::Path;

/// Path checks shared by the pre-draft guard and the handler.
pub(in crate::app) fn validate_export_path(path: &Path) -> Result<(), OperationError> {
    validate_path_text(path, "Export")?;
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
        let view: ExportView = self.export_view(selection_only)?;
        let summary =
            gltf::export_native_new(path, self.history.scene(), &view.instances(), may_install)
                .map_err(|error| {
                    OperationError::GltfExport(match error {
                        GltfError::Io { source, .. }
                            if source.kind() == std::io::ErrorKind::AlreadyExists =>
                        {
                            ExportError::DestinationExists
                        }
                        GltfError::Io { source, .. } => ExportError::Io(source.kind()),
                        GltfError::NothingToExport => ExportError::NothingToExport,
                        GltfError::Cancelled => ExportError::Cancelled,
                        error @ (GltfError::SessionUvs | GltfError::SessionMaterial) => {
                            ExportError::Unsupported(error.to_string())
                        }
                        error => ExportError::Invalid(error.to_string()),
                    })
                })?;
        Ok(OperationReply::GltfExported {
            objects: summary.objects,
            vertices: summary.vertices,
            triangles: summary.triangles,
        })
    }
}
