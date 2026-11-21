use super::{NativeError, NativeId, NativeScene};
use crate::scene::NodeId;
use std::collections::BTreeSet;

const MAX_LAYERS: u32 = 32;
pub const MAX_DISPLAY_LAYER_MEMBERS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayLayerEdit {
    Create { members: Vec<NodeId> },
    Assign { layer: u32, members: Vec<NodeId> },
    Visibility { layer: u32, visible: bool },
    Delete { layer: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayLayer {
    visible: bool,
    members: BTreeSet<NativeId>,
}

impl DisplayLayer {
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn contains(&self, id: NativeId) -> bool {
        self.members.contains(&id)
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub(super) fn remove_members(&mut self, gone: &[NativeId]) {
        self.members.retain(|id| !gone.contains(id));
    }
}

impl NativeScene {
    pub fn display_layers(&self) -> impl Iterator<Item = (u32, &DisplayLayer)> {
        self.display_layers.iter().map(|(&id, layer)| (id, layer))
    }

    pub(super) fn layer_member_visible(&self, id: NativeId) -> bool {
        !self
            .display_layers
            .values()
            .any(|layer| !layer.visible && layer.contains(id))
    }

    pub fn edit_display_layer(&mut self, edit: DisplayLayerEdit) -> Result<(), NativeError> {
        self.structural_edit()?;
        match edit {
            DisplayLayerEdit::Create { members } => {
                let layer = (1..=MAX_LAYERS)
                    .find(|id| !self.display_layers.contains_key(id))
                    .ok_or(NativeError::InvalidDisplayLayer(
                        "Display layers are limited to 32",
                    ))?;
                let members = self.validate_display_layer_members(members, true)?;
                self.transfer_display_layer_members(&members);
                self.display_layers.insert(
                    layer,
                    DisplayLayer {
                        visible: true,
                        members,
                    },
                );
            }
            DisplayLayerEdit::Assign { layer, members } => {
                if members.is_empty() {
                    return Err(NativeError::InvalidDisplayLayer(
                        "Choose at least one transform",
                    ));
                }
                if layer != 0 && !self.display_layers.contains_key(&layer) {
                    return Err(NativeError::InvalidDisplayLayer("Unknown display layer"));
                }
                let members = self.validate_display_layer_members(members, layer != 0)?;
                self.transfer_display_layer_members(&members);
                if let Some(layer) = self.display_layers.get_mut(&layer) {
                    layer.members.extend(members);
                }
            }
            DisplayLayerEdit::Visibility { layer, visible } => {
                self.display_layers
                    .get_mut(&layer)
                    .ok_or(NativeError::InvalidDisplayLayer(
                        "Choose an existing named display layer",
                    ))?
                    .visible = visible;
            }
            DisplayLayerEdit::Delete { layer } => {
                self.display_layers
                    .remove(&layer)
                    .ok_or(NativeError::InvalidDisplayLayer(
                        "Choose an existing named display layer",
                    ))?;
            }
        }
        Ok(())
    }

    fn validate_display_layer_members(
        &self,
        members: Vec<NodeId>,
        named_layer: bool,
    ) -> Result<BTreeSet<NativeId>, NativeError> {
        if members.len() > MAX_DISPLAY_LAYER_MEMBERS {
            return Err(NativeError::InvalidDisplayLayer(
                "Display layer membership is limited to 512 transforms",
            ));
        }
        let ids: BTreeSet<_> = members
            .iter()
            .map(|&id| self.node(id).map(|node| node.transform_id()))
            .collect::<Result<_, _>>()?;
        if ids.len() != members.len() {
            return Err(NativeError::InvalidDisplayLayer(
                "Display layer membership repeats a transform",
            ));
        }
        if named_layer {
            let current = self
                .display_layers
                .values()
                .map(DisplayLayer::member_count)
                .sum::<usize>();
            let new = ids
                .iter()
                .filter(|id| {
                    !self
                        .display_layers
                        .values()
                        .any(|layer| layer.contains(**id))
                })
                .count();
            if current + new > MAX_DISPLAY_LAYER_MEMBERS {
                return Err(NativeError::InvalidDisplayLayer(
                    "Named layers can hold at most 512 transforms in total",
                ));
            }
        }
        Ok(ids)
    }

    fn transfer_display_layer_members(&mut self, members: &BTreeSet<NativeId>) {
        for layer in self.display_layers.values_mut() {
            layer.members.retain(|member| !members.contains(member));
        }
    }
}
