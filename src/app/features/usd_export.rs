//! Export All / Export Selection as USD ASCII (`.usda`), shared with MCP `export_usd`.

use super::export_common::validate_path_text;
use crate::app::Editor;
use crate::editor_operations::{ExportError, OperationError, OperationReply};
use azteccraft::usd::{self, UsdError};
use std::path::Path;

/// Path checks shared by the pre-draft guard and the handler.
pub(in crate::app) fn validate_export_path(path: &Path) -> Result<(), OperationError> {
    validate_path_text(path, "Export")?;
    if !usd::is_usda_path(path) {
        return Err(OperationError::InvalidInput(
            "Export path must end in .usda".into(),
        ));
    }
    Ok(())
}

impl Editor {
    /// Writes displayed polygon geometry to a new `.usda` file: every instance, or the instances
    /// under the selected objects. Reads the committed scene and changes nothing in it.
    pub(in crate::app) fn export_usd(
        &mut self,
        path: &Path,
        selection_only: bool,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        validate_export_path(path)?;
        let view = self.export_view(selection_only)?;
        let summary =
            usd::export_native_new(path, self.history.scene(), &view.instances(), may_install)
                .map_err(|error| {
                    OperationError::UsdExport(match error {
                        UsdError::Io { source, .. }
                            if source.kind() == std::io::ErrorKind::AlreadyExists =>
                        {
                            ExportError::DestinationExists
                        }
                        UsdError::Io { source, .. } => ExportError::Io(source.kind()),
                        UsdError::NothingToExport => ExportError::NothingToExport,
                        UsdError::Cancelled => ExportError::Cancelled,
                        error @ (UsdError::SessionUvs | UsdError::SessionMaterial) => {
                            ExportError::Unsupported(error.to_string())
                        }
                        error => ExportError::Invalid(error.to_string()),
                    })
                })?;
        Ok(OperationReply::UsdExported {
            objects: summary.objects,
            vertices: summary.vertices,
            faces: summary.faces,
        })
    }
}
