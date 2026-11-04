use super::{NativeError, NativeId, NativeScene, source::SourceRetention};
use crate::scene::NodeId;
use serde::Serialize;
use std::{fmt, ops::Range};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceIssue {
    MissingMembership,
    OverlappingMembership,
    UnsupportedShader,
    UnsupportedGraph,
    AmbiguousInstance,
    UnsupportedSettings,
    UvMismatch,
    MalformedData,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AppearanceIssues {
    pub missing_membership: bool,
    pub overlapping_membership: bool,
    pub unsupported_shader: bool,
    pub unsupported_graph: bool,
    pub ambiguous_instance: bool,
    pub unsupported_settings: bool,
    pub uv_mismatch: bool,
    pub malformed_data: bool,
    pub evaluation_unavailable: bool,
    pub quadratic_filtering_unproved: bool,
    pub color_processor_unavailable: bool,
}

impl AppearanceIssues {
    fn insert(&mut self, issue: AppearanceIssue) {
        match issue {
            AppearanceIssue::MissingMembership => self.missing_membership = true,
            AppearanceIssue::OverlappingMembership => self.overlapping_membership = true,
            AppearanceIssue::UnsupportedShader => self.unsupported_shader = true,
            AppearanceIssue::UnsupportedGraph => self.unsupported_graph = true,
            AppearanceIssue::AmbiguousInstance => self.ambiguous_instance = true,
            AppearanceIssue::UnsupportedSettings => self.unsupported_settings = true,
            AppearanceIssue::UvMismatch => self.uv_mismatch = true,
            AppearanceIssue::MalformedData => self.malformed_data = true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceState {
    Unbound,
    Unavailable,
    CheckedViewerSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AppearanceSummary {
    pub state: AppearanceState,
    pub shapes: usize,
    pub checked_source_bindings: usize,
    pub unavailable: usize,
    pub unbound: usize,
    pub issues: AppearanceIssues,
}

impl AppearanceSummary {
    fn empty() -> Self {
        Self {
            state: AppearanceState::Unbound,
            shapes: 0,
            checked_source_bindings: 0,
            unavailable: 0,
            unbound: 0,
            issues: AppearanceIssues {
                evaluation_unavailable: true,
                quadratic_filtering_unproved: true,
                color_processor_unavailable: true,
                ..AppearanceIssues::default()
            },
        }
    }

    fn add(&mut self, binding: Option<&ShapeAppearance>) {
        self.shapes += 1;
        match binding.map(|binding| &binding.binding) {
            None => self.unbound += 1,
            Some(Ok(_)) => self.checked_source_bindings += 1,
            Some(Err(issue)) => {
                self.unavailable += 1;
                self.issues.insert(*issue);
            }
        }
        self.state = if self.unbound == self.shapes {
            AppearanceState::Unbound
        } else if self.checked_source_bindings == self.shapes {
            AppearanceState::CheckedViewerSource
        } else {
            AppearanceState::Unavailable
        };
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct SourceAppearance {
    pub shapes: Vec<ShapeAppearance>,
    pub materials: Vec<SourceMaterial>,
    pub diffuse: Vec<DiffuseBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiffuseBinding {
    pub shader: usize,
    pub span: SourceSpan,
    pub shapes: Vec<NativeId>,
}

impl DiffuseBinding {
    pub(super) fn original(&self, source: &SourceRetention) -> Result<f64, NativeError> {
        self.span.validate(source)?;
        let word: [u8; 8] = source.bytes[self.span.value.clone()]
            .try_into()
            .map_err(|_| NativeError::InvalidRetention)?;
        let value = f64::from_be_bytes(word);
        if !value.is_finite() {
            return Err(NativeError::InvalidRetention);
        }
        Ok(value)
    }
}

#[derive(Clone)]
pub(super) struct MaterialDiffuse {
    pub binding: usize,
    pub value: f64,
    pub replaced: bool,
}

impl PartialEq for MaterialDiffuse {
    fn eq(&self, other: &Self) -> bool {
        self.binding == other.binding
            && self.value.to_bits() == other.value.to_bits()
            && self.replaced == other.replaced
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialShader {
    Phong,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialInspection {
    pub shader: MaterialShader,
    pub diffuse: f64,
    pub shared_users: usize,
    pub editing_available: bool,
    pub rendering_available: bool,
}

impl fmt::Debug for SourceAppearance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceAppearance")
            .field("shapes", &self.shapes.len())
            .field("materials", &self.materials.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ShapeAppearance {
    pub shape: NativeId,
    pub source_group: usize,
    pub binding: Result<SourceSurfaceBinding, AppearanceIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceSpan {
    pub leaf: Range<usize>,
    pub value: Range<usize>,
}

impl SourceSpan {
    fn validate(&self, source: &SourceRetention) -> Result<(), NativeError> {
        if self.leaf.start > self.value.start
            || self.value.end > self.leaf.end
            || self.value.start > self.value.end
            || self.leaf.end > source.bytes.len()
        {
            return Err(NativeError::InvalidRetention);
        }
        let index = source
            .records
            .partition_point(|range| range.start <= self.leaf.start);
        if index == 0
            || source.records[index - 1].start > self.leaf.start
            || source.records[index - 1].end < self.leaf.end
        {
            return Err(NativeError::InvalidRetention);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceSurfaceBinding {
    pub faces: Range<usize>,
    pub instance_parent: NativeId,
    pub component: SourceSpan,
    pub group_id: usize,
    pub group_id_link: SourceSpan,
    pub membership_link: SourceSpan,
    pub group_link: SourceSpan,
    pub group_color_link: SourceSpan,
    pub uv_name: SourceSpan,
    pub explicit_uv_name: SourceSpan,
    pub active_uv_name: SourceSpan,
    pub explicit_uv: SourceSpan,
    pub packed_uv: SourceSpan,
    pub corner_ids: SourceSpan,
    pub material: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InputProperty {
    TexturePath,
    ColorSpace,
    Diffuse,
    Ambient,
    Specular,
    Reflectivity,
    PlacementCompoundDefaults,
    TextureCompoundDefaults,
    PhongTransparencyDefault,
    ColorManagementDefault2025,
    ColorConfigurationEnabled,
    ColorConfiguration,
    WorkingSpace,
    ShadingGroupHistoryInterest2026,
    ShadingGroupRenderableOnlySet2026,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InputEvidence {
    Saved {
        property: InputProperty,
        span: SourceSpan,
    },
    SchemaDefault(InputProperty),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceMaterial {
    pub owners: [usize; 4],
    pub shader_link: SourceSpan,
    pub texture_link: SourceSpan,
    pub placement_links: [SourceSpan; 11],
    pub color_links: [SourceSpan; 4],
    pub inputs: Vec<InputEvidence>,
    pub message_links: Vec<SourceMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceMessage {
    pub span: SourceSpan,
    pub mode: u8,
    pub source: Range<usize>,
    pub destination: Range<usize>,
}

impl SourceAppearance {
    pub(super) fn metadata_bytes(&self) -> Result<usize, NativeError> {
        let product =
            |count: usize, width: usize| count.checked_mul(width).ok_or(NativeError::ResourceLimit);
        let mut bytes = std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>();
        for amount in [
            product(
                self.shapes.capacity(),
                std::mem::size_of::<ShapeAppearance>(),
            )?,
            product(
                self.materials.capacity(),
                std::mem::size_of::<SourceMaterial>(),
            )?,
            product(
                self.diffuse.capacity(),
                std::mem::size_of::<DiffuseBinding>(),
            )?,
        ] {
            bytes = bytes
                .checked_add(amount)
                .ok_or(NativeError::ResourceLimit)?;
        }
        for material in &self.materials {
            for amount in [
                product(
                    material.inputs.capacity(),
                    std::mem::size_of::<InputEvidence>(),
                )?,
                product(
                    material.message_links.capacity(),
                    std::mem::size_of::<SourceMessage>(),
                )?,
            ] {
                bytes = bytes
                    .checked_add(amount)
                    .ok_or(NativeError::ResourceLimit)?;
            }
        }
        for binding in &self.diffuse {
            bytes = bytes
                .checked_add(product(
                    binding.shapes.capacity(),
                    std::mem::size_of::<NativeId>(),
                )?)
                .ok_or(NativeError::ResourceLimit)?;
        }
        Ok(bytes)
    }

    /// The shape entries of the shapes still in `scene`; removed shapes keep their entries.
    fn kept_shapes<'a>(
        &'a self,
        scene: &'a NativeScene,
    ) -> impl Iterator<Item = &'a ShapeAppearance> {
        self.shapes
            .iter()
            .filter(|entry| scene.removed.binary_search(&entry.shape).is_err())
    }

    pub(super) fn validate(
        &self,
        scene: &NativeScene,
        source: &SourceRetention,
    ) -> Result<(), NativeError> {
        let kept: Vec<_> = self.kept_shapes(scene).collect();
        if kept.len() != scene.shapes.len() {
            return Err(NativeError::InvalidRetention);
        }
        for (entry, shape) in kept.into_iter().zip(&scene.shapes) {
            if entry.shape != shape.id || entry.source_group >= source.records.len() {
                return Err(NativeError::InvalidRetention);
            }
            let Ok(binding) = &entry.binding else {
                continue;
            };
            let corners = shape.mesh.faces().iter().try_fold(0usize, |sum, face| {
                sum.checked_add(face.outer.len())
                    .ok_or(NativeError::InvalidRetention)
            })?;
            if binding.faces != (0..shape.mesh.faces().len())
                || binding.material >= self.materials.len()
                || binding.group_id >= source.records.len()
                || binding.corner_ids.value.len()
                    != corners
                        .checked_mul(4)
                        .ok_or(NativeError::InvalidRetention)?
                || binding.explicit_uv.value.len() != binding.packed_uv.value.len()
                || !scene.dag.iter().any(|edge| {
                    edge.parent == Some(binding.instance_parent) && edge.child == shape.id
                })
            {
                return Err(NativeError::InvalidRetention);
            }
            for span in [
                &binding.component,
                &binding.group_id_link,
                &binding.membership_link,
                &binding.group_link,
                &binding.group_color_link,
                &binding.uv_name,
                &binding.explicit_uv_name,
                &binding.active_uv_name,
                &binding.explicit_uv,
                &binding.packed_uv,
                &binding.corner_ids,
            ] {
                span.validate(source)?;
            }
            if source.bytes[binding.explicit_uv.value.clone()]
                != source.bytes[binding.packed_uv.value.clone()]
            {
                return Err(NativeError::InvalidRetention);
            }
            let uv = &source.bytes[binding.packed_uv.value.clone()];
            if !uv.len().is_multiple_of(8)
                || uv
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|word| !f32::from_be_bytes(*word).is_finite())
                || source.bytes[binding.corner_ids.value.clone()]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|word| u32::from_be_bytes(*word) as usize >= uv.len() / 8)
                || source.bytes[binding.uv_name.value.clone()]
                    != source.bytes[binding.explicit_uv_name.value.clone()]
                || source.bytes[binding.uv_name.value.clone()]
                    != source.bytes[binding.active_uv_name.value.clone()]
                || binding.uv_name.value.is_empty()
            {
                return Err(NativeError::InvalidRetention);
            }
        }
        for material in &self.materials {
            if material
                .owners
                .iter()
                .any(|index| *index >= source.records.len())
            {
                return Err(NativeError::InvalidRetention);
            }
            for span in [&material.shader_link, &material.texture_link]
                .into_iter()
                .chain(&material.placement_links)
                .chain(&material.color_links)
            {
                span.validate(source)?;
            }
            for input in &material.inputs {
                if let InputEvidence::Saved { span, .. } = input {
                    span.validate(source)?;
                }
            }
            for message in &material.message_links {
                message.span.validate(source)?;
                if message.mode > 1
                    || [&message.source, &message.destination].iter().any(|range| {
                        range.start < message.span.value.start
                            || range.end > message.span.value.end
                            || range.start >= range.end
                    })
                {
                    return Err(NativeError::InvalidRetention);
                }
            }
        }
        for (index, binding) in self.diffuse.iter().enumerate() {
            binding.original(source)?;
            if binding.shapes.is_empty()
                || self.diffuse[..index]
                    .iter()
                    .any(|other| other.shader == binding.shader)
            {
                return Err(NativeError::InvalidRetention);
            }
            let mut users = 0usize;
            for shape in self.kept_shapes(scene) {
                let Ok(surface) = &shape.binding else {
                    continue;
                };
                let material = &self.materials[surface.material];
                if material.owners[1] != binding.shader {
                    continue;
                }
                if !binding.shapes.contains(&shape.shape) || !material.inputs.iter().any(|input| {
                    matches!(input, InputEvidence::Saved { property: InputProperty::Diffuse, span } if *span == binding.span)
                }) {
                    return Err(NativeError::InvalidRetention);
                }
                users += 1;
            }
            if users != binding.shapes.len() {
                return Err(NativeError::InvalidRetention);
            }
        }
        Ok(())
    }
}

impl NativeScene {
    fn material_index(&self, shape: NativeId) -> Result<usize, NativeError> {
        self.shape(shape)?;
        self.source
            .as_ref()
            .and_then(|source| source.appearance.as_ref())
            .and_then(|appearance| {
                appearance
                    .diffuse
                    .iter()
                    .position(|binding| binding.shapes.contains(&shape))
            })
            .ok_or(NativeError::UnsupportedEdit)
    }

    pub fn material_for_shape(&self, shape: NativeId) -> Result<MaterialInspection, NativeError> {
        let index = self.material_index(shape)?;
        self.validate_materials()?;
        let binding = &self
            .source
            .as_ref()
            .unwrap()
            .appearance
            .as_ref()
            .unwrap()
            .diffuse[index];
        Ok(MaterialInspection {
            shader: MaterialShader::Phong,
            diffuse: self.materials[index].value,
            shared_users: binding.shapes.len(),
            editing_available: true,
            rendering_available: false,
        })
    }

    pub fn set_material_diffuse(&mut self, shape: NativeId, value: f64) -> Result<(), NativeError> {
        if !value.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if value < 0.0 {
            return Err(NativeError::UnsupportedEdit);
        }
        let index = self.material_index(shape)?;
        self.validate_materials()?;
        if self.materials[index].value.to_bits() == value.to_bits() {
            return Ok(());
        }
        let source = self.source.as_ref().unwrap();
        let original = source.appearance.as_ref().unwrap().diffuse[index].original(source)?;
        self.materials[index].value = value;
        self.materials[index].replaced = value.to_bits() != original.to_bits();
        Ok(())
    }

    pub(super) fn material_bytes(&self) -> Result<usize, NativeError> {
        self.materials
            .capacity()
            .checked_mul(std::mem::size_of::<MaterialDiffuse>())
            .ok_or(NativeError::ResourceLimit)
    }

    pub(super) fn validate_materials(&self) -> Result<(), NativeError> {
        let Some((source, appearance)) = self.source.as_ref().and_then(|source| {
            source
                .appearance
                .as_ref()
                .map(|appearance| (source, appearance))
        }) else {
            return if self.materials.is_empty() {
                Ok(())
            } else {
                Err(NativeError::InvalidRetention)
            };
        };
        if self.materials.len() != appearance.diffuse.len() {
            return Err(NativeError::InvalidRetention);
        }
        for (index, (value, binding)) in self.materials.iter().zip(&appearance.diffuse).enumerate()
        {
            let original = binding.original(source)?;
            if value.binding != index
                || !value.value.is_finite()
                || value.replaced != (value.value.to_bits() != original.to_bits())
                || (value.replaced && value.value < 0.0)
            {
                return Err(NativeError::InvalidRetention);
            }
        }
        Ok(())
    }

    fn appearance_binding(&self, shape: NativeId) -> Option<&ShapeAppearance> {
        let appearance = self.source.as_ref()?.appearance.as_ref()?;
        appearance
            .shapes
            .binary_search_by_key(&shape, |entry| entry.shape)
            .ok()
            .map(|index| &appearance.shapes[index])
    }

    pub fn appearance_readiness(&self) -> AppearanceSummary {
        let mut summary = AppearanceSummary::empty();
        for shape in &self.shapes {
            summary.add(self.appearance_binding(shape.id));
        }
        summary
    }

    pub fn shape_appearance_readiness(
        &self,
        shape: NativeId,
    ) -> Result<AppearanceSummary, NativeError> {
        self.shape(shape)?;
        let mut summary = AppearanceSummary::empty();
        summary.add(self.appearance_binding(shape));
        Ok(summary)
    }

    pub fn transform_appearance_readiness(
        &self,
        id: NodeId,
    ) -> Result<Option<AppearanceSummary>, NativeError> {
        let transform = self.node(id)?.transform_id;
        let mut summary = AppearanceSummary::empty();
        for edge in &self.dag {
            if edge.parent == Some(transform) && self.shape(edge.child).is_ok() {
                summary.add(self.appearance_binding(edge.child));
            }
        }
        Ok((summary.shapes != 0).then_some(summary))
    }
}
