use crate::app::*;
use crate::editor_operations::RecoveryFailure;
use azteccraft::document;

impl Editor {
    /// Enable recovery files in `dir`. Called only by the host (`--recovery-dir`), never by an
    /// operation, so a remote client cannot choose where files are written. The folder must be on
    /// a local disk (see the `recovery` module notes).
    #[cfg(any(test, feature = "mcp"))]
    pub(crate) fn enable_recovery(
        &mut self,
        dir: std::path::PathBuf,
    ) -> Result<(), azteccraft::recovery::RecoveryError> {
        self.recovery = Some(azteccraft::recovery::RecoveryStore::new(
            dir,
            azteccraft::recovery::DEFAULT_MAX_BACKUPS,
            std::process::id(),
        )?);
        Ok(())
    }

    /// Encode the committed scene and write one recovery file. It never commits a draft, never
    /// touches `files` or `history`, and never names the scene file as a destination.
    pub(in crate::app) fn autosave_now(&mut self) -> Result<OperationReply, OperationError> {
        if self.history.active_manipulation().is_some() {
            return Err(OperationError::BusyManipulation);
        }
        let store = self
            .recovery
            .as_ref()
            .ok_or(OperationError::Recovery(RecoveryFailure::Disabled))?;
        let bytes = document::encode_recovery(self.history.scene())
            .map_err(|error| OperationError::Recovery(RecoveryFailure::Encode(error)))?;
        let stem = self.files.scene_stem().unwrap_or("untitled");
        let outcome = store
            .write(stem, &bytes)
            .map_err(|error| OperationError::Recovery(RecoveryFailure::Store(error)))?;
        Ok(OperationReply::Autosaved {
            sequence: outcome.entry.sequence,
            bytes: outcome.entry.len,
            pruned: outcome.pruned,
            prune_failures: outcome.prune_failures,
        })
    }

    /// A folder that cannot be listed is an error, never `files: 0`.
    pub(in crate::app) fn recovery_status(&self) -> Result<OperationReply, OperationError> {
        let files = match &self.recovery {
            Some(store) => Some(
                store
                    .list()
                    .map_err(|error| OperationError::Recovery(RecoveryFailure::Store(error)))?
                    .len(),
            ),
            None => None,
        };
        Ok(OperationReply::RecoveryStatus {
            enabled: self.recovery.is_some(),
            max_backups: self
                .recovery
                .as_ref()
                .map(azteccraft::recovery::RecoveryStore::max_backups),
            files: files.unwrap_or(0),
        })
    }
}

#[cfg(test)]
#[path = "recovery/tests.rs"]
mod tests;
