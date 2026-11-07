use crate::app::*;
use azteccraft::viewport::NdcRect;

/// What a marquee drag does by modifier: none replaces, Shift toggles, Ctrl or Command removes,
/// Shift+Ctrl or Shift+Command adds. The click mapping with Replace for a plain drag; the Marquee
/// page of the Maya docs is silent on Shift, so Shift toggling is PROVISIONAL.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the native marquee release calls it, added next")
)]
pub(in crate::app) fn marquee_mode(modifiers: egui::Modifiers) -> MarqueeMode {
    match (modifiers.shift, modifiers.ctrl || modifiers.command) {
        (false, false) => MarqueeMode::Replace,
        (true, false) => MarqueeMode::Toggle,
        (false, true) => MarqueeMode::Remove,
        (true, true) => MarqueeMode::Add,
    }
}

impl Editor {
    /// The objects whose visible mesh overlaps `rect` (any overlap, no occlusion: PROVISIONAL),
    /// mapped like a click in the current selection mode, in instance order. Several hits can map
    /// to one target (Hierarchy mode); the selection mutators collapse the repeats.
    fn pick_object_targets(&self, rect: NdcRect) -> Result<Vec<TransformTarget>, OperationError> {
        // Same literal as the click and frame paths; the #22 VIEW_BUDGET lift replaces them all.
        let view = ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024)
            .map_err(|error| OperationError::View(error.to_string()))?;
        let hits = self
            .camera
            .pick_rect(&view, rect, self.aspect)
            .map_err(|error| OperationError::View(error.to_string()))?;
        let mut targets = Vec::with_capacity(hits.len());
        for index in hits {
            let instance = &view.instances()[index];
            let (node, path) = self.object_target_for_hit(FaceTarget {
                transform: instance.transform(),
                shape: instance.shape(),
                path: instance.path().to_vec(),
                face_index: 0,
            })?;
            targets.push(TransformTarget { node, path });
        }
        Ok(targets)
    }

    /// Combines the marquee's objects with the selection. A plain marquee with no hit clears it;
    /// a modified one with no hit changes nothing. Over the cap the selection stays as it was.
    pub(in crate::app) fn select_in_rect(
        &mut self,
        rect: NdcRect,
        mode: MarqueeMode,
    ) -> Result<OperationReply, OperationError> {
        let targets = self.pick_object_targets(rect)?;
        let mut next = self.selection.clone();
        let full = |error: azteccraft::selection::SelectionFull| {
            OperationError::InvalidInput(error.to_string())
        };
        match mode {
            MarqueeMode::Replace => next.replace_with(targets).map_err(full)?,
            MarqueeMode::Add => next.extend_absent(targets).map_err(full)?,
            MarqueeMode::Remove => {
                next.remove_all(&targets);
            }
            MarqueeMode::Toggle => next.toggle_all(&targets).map_err(full)?,
        }
        let changed = next != self.selection;
        if changed {
            self.selection = next;
            self.clear_remembered_components();
            self.sync_fields();
        }
        Ok(if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        })
    }
}

#[cfg(test)]
mod tests;
