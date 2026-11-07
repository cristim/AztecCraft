//! Application preferences as editor operations: the compatibility version and Reset of a
//! blocked file. Both are session settings like `SetGridVisible`: they never touch the scene,
//! the undo stack or the scene token.

use crate::app::*;
use azteccraft::compatibility::MayaVersion;
use azteccraft::preferences::{Preferences, ResetOutcome, Store};

/// The modeless Preferences window's UI state. `pending` is an unsaved picker value; it is not a
/// scene draft, so it never counts toward `has_draft()` or `is_dirty()`.
#[derive(Default)]
pub(in crate::app) struct PreferencesWindow {
    #[cfg_attr(not(any(test, feature = "mcp")), allow(dead_code))]
    pub(in crate::app) pending: Option<MayaVersion>,
}

impl Editor {
    /// A change is persisted first when the store is writable; a failed write leaves value and
    /// file as they were. A blocked or session-only store applies the value to this session only.
    pub(in crate::app) fn set_compatibility_version(
        &mut self,
        version: MayaVersion,
    ) -> Result<OperationReply, OperationError> {
        if self.preferences.maya_compatibility_version == version {
            return Ok(OperationReply::Unchanged);
        }
        let next = Preferences {
            maya_compatibility_version: version,
        };
        if matches!(self.preferences_store, Store::Writable { .. }) {
            self.preferences_store
                .save(&next)
                .map_err(OperationError::Preferences)?;
        }
        self.preferences = next;
        Ok(OperationReply::Changed)
    }

    pub(in crate::app) fn reset_preferences(&mut self) -> Result<OperationReply, OperationError> {
        match self
            .preferences_store
            .reset()
            .map_err(OperationError::Preferences)?
        {
            ResetOutcome::Unchanged => Ok(OperationReply::Unchanged),
            ResetOutcome::MovedAside { .. } | ResetOutcome::NothingToMove => {
                self.preferences = Preferences::default();
                Ok(OperationReply::Changed)
            }
        }
    }
}

#[cfg(test)]
mod tests;
