use super::{
    BinaryError, ErrorKind, admission::Budget, canonical, decode_with_retained_bytes, resource,
};
use crate::maya_binary::container;
use crate::native_scene::{
    NativeId, NativeScene, NativeTransformNode,
    appearance::SourceAppearance,
    source::{SourceFormat, TransformField},
};
use insert::{Insertion, check_inverse, splice};
use std::sync::Arc;

const FIELDS: [super::super::TransformField; 5] = [
    super::super::TransformField::Translation,
    super::super::TransformField::Rotation,
    super::super::TransformField::Scale,
    super::super::TransformField::RotatePivot,
    super::super::TransformField::ScalePivot,
];

fn unsupported() -> BinaryError {
    BinaryError {
        kind: ErrorKind::Unsupported,
        offset: None,
    }
}

fn same_node_structure(a: &NativeTransformNode, b: &NativeTransformNode) -> bool {
    a.id == b.id
        && a.transform_id == b.transform_id
        && a.transform_uuid == b.transform_uuid
        && a.name == b.name
        && a.restrictions == b.restrictions
        && a.unsupported_transform == b.unsupported_transform
        && a.extras == b.extras
        && a.view == b.view
}

fn same_structure(a: &NativeScene, b: &NativeScene) -> bool {
    a.units == b.units
        && a.dag == b.dag
        && a.shapes == b.shapes
        && a.curves == b.curves
        && a.session_materials == b.session_materials
        && a.nodes.len() == b.nodes.len()
        && a.nodes
            .iter()
            .zip(&b.nodes)
            .all(|(a, b)| same_node_structure(a, b))
}

fn check_appearance_metadata(
    expected: &NativeScene,
    actual: &NativeScene,
) -> Result<(), BinaryError> {
    let expected = expected
        .source
        .as_ref()
        .and_then(|source| source.appearance.as_deref());
    let actual = actual
        .source
        .as_ref()
        .and_then(|source| source.appearance.as_deref());
    if expected != actual {
        return Err(unsupported());
    }
    Ok(())
}

/// Appearance facts that hold in any byte or chunk coordinates, for a reparse of spliced bytes.
fn same_appearance_shape(a: Option<&SourceAppearance>, b: Option<&SourceAppearance>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.shapes.len() == b.shapes.len()
                && a.materials.len() == b.materials.len()
                && a.diffuse.len() == b.diffuse.len()
                && a.shapes.iter().zip(&b.shapes).all(|(x, y)| {
                    x.shape == y.shape
                        && match (&x.binding, &y.binding) {
                            (Ok(x), Ok(y)) => {
                                x.faces == y.faces
                                    && x.instance_parent == y.instance_parent
                                    && x.material == y.material
                            }
                            (Err(x), Err(y)) => x == y,
                            _ => false,
                        }
                })
        }
        _ => false,
    }
}

fn retained_material_metadata(scene: &NativeScene) -> Result<usize, BinaryError> {
    let appearance = scene
        .source
        .as_ref()
        .and_then(|source| source.appearance.as_ref())
        .map(|appearance| appearance.metadata_bytes())
        .transpose()
        .map_err(|error| canonical(error, None))?
        .unwrap_or(0);
    appearance
        .checked_add(
            scene
                .material_bytes()
                .map_err(|error| canonical(error, None))?,
        )
        .ok_or_else(resource)
}

pub fn encode(scene: &NativeScene) -> Result<Vec<u8>, BinaryError> {
    // Without a binary source `encode_with` reports the refusal.
    let planned = match scene
        .source
        .as_ref()
        .filter(|source| source.format == SourceFormat::Binary)
    {
        Some(source) => plan::plan(scene, source, plan::INSERTABLE)?,
        None => plan::Planned::default(),
    };
    encode_with(scene, &planned.insertions, &planned.elided)
}

/// `plan` lists default leaves to splice in after the word patches. Production plans come from
/// `plan::plan` over the `INSERTABLE` gate; tests hand-build plans.
fn encode_with(
    scene: &NativeScene,
    plan: &[Insertion],
    elided: &[(NativeId, TransformField)],
) -> Result<Vec<u8>, BinaryError> {
    if !scene.curves.is_empty() {
        return Err(unsupported());
    }
    let source = scene
        .source
        .as_ref()
        .filter(|s| s.format == SourceFormat::Binary)
        .ok_or_else(unsupported)?;
    {
        let retained_appearance = retained_material_metadata(scene)?;
        let original = decode_with_retained_bytes(source.bytes.clone(), 0, retained_appearance)?;
        if !same_structure(scene, &original) || original.source.as_ref() != Some(source) {
            return Err(unsupported());
        }
        for (node, old) in scene.nodes.iter().zip(&original.nodes) {
            for field in FIELDS {
                for (scalar, before) in node.field(field).iter().zip(old.field(field)) {
                    if scalar.origin != before.origin || (!scalar.replaced && scalar != before) {
                        return Err(unsupported());
                    }
                }
            }
        }
        scene.validate().map_err(|e| canonical(e, None))?;
    }
    let mut budget = Budget::new(
        source
            .bytes
            .len()
            .checked_add(retained_material_metadata(scene)?)
            .ok_or_else(resource)?,
    )?;
    // Edited leaves the file does not have: inserted below, or elided because every word is +0.0.
    // The word patches have no leaf for either. The patched copy still lacks the inserted ones.
    let mut planned = budget.vector::<(NativeId, TransformField)>(plan.len())?;
    planned.extend(plan.iter().map(|i| (i.owner, i.field)));
    planned.sort_unstable_by_key(|entry| entry.0);
    let mut elided_fields = budget.vector::<(NativeId, TransformField)>(elided.len())?;
    elided_fields.extend_from_slice(elided);
    elided_fields.sort_unstable_by_key(|entry| entry.0);
    let mut output = budget.vector::<u8>(source.bytes.len())?;
    output.extend_from_slice(&source.bytes);
    for node in &scene.nodes {
        for field in FIELDS {
            for (component, scalar) in node.field(field).iter().enumerate() {
                if !scalar.replaced {
                    continue;
                }
                if scalar.origin.is_none()
                    && (has_field(&planned, node.transform_id, field)
                        || has_field(&elided_fields, node.transform_id, field))
                {
                    continue;
                }
                let origin = scalar.origin.as_ref().ok_or_else(unsupported)?;
                let leaf = source
                    .binary_leaf(origin, node.transform_id, field, component)
                    .map_err(|e| canonical(e, None))?;
                let start = leaf
                    .numeric
                    .start
                    .checked_add(component * 8)
                    .ok_or_else(resource)?;
                let end = start.checked_add(8).ok_or_else(resource)?;
                let word = scalar
                    .internal(
                        Some(source),
                        &scene.units,
                        node.transform_id,
                        field,
                        component,
                    )
                    .map_err(|e| canonical(e, None))?;
                output
                    .get_mut(start..end)
                    .ok_or_else(unsupported)?
                    .copy_from_slice(&word.to_be_bytes());
            }
        }
    }
    for value in &scene.materials {
        if !value.replaced {
            continue;
        }
        let binding = source
            .appearance
            .as_ref()
            .and_then(|appearance| appearance.diffuse.get(value.binding))
            .ok_or_else(unsupported)?;
        binding
            .original(source)
            .map_err(|error| canonical(error, None))?;
        output
            .get_mut(binding.span.value.clone())
            .ok_or_else(unsupported)?
            .copy_from_slice(&value.value.to_be_bytes());
    }
    let retained = source
        .bytes
        .len()
        .checked_add(output.capacity())
        .ok_or_else(resource)?;
    if retained > 2 * crate::maya_binary::container::MAX_BYTES {
        return Err(resource());
    }
    // Reparse while retaining both the live source and the returned output buffer.
    budget.charge(output.len())?;
    let retained_appearance = retained_material_metadata(scene)?;
    let parsed =
        decode_with_retained_bytes(Arc::from(output.as_slice()), retained, retained_appearance)?;
    verify(scene, source, &parsed, &planned, Appearance::Exact)?;
    // Its Arc copy of `output` is not counted in `retained` below, so release it first.
    drop(parsed);
    if plan.is_empty() {
        return Ok(output);
    }
    let spliced = splice(&output, plan, &mut budget)?;
    check_inverse(&spliced, &output, plan, &mut budget)?;
    let retained = retained
        .checked_add(spliced.capacity())
        .filter(|retained| *retained <= 2 * crate::maya_binary::container::MAX_BYTES)
        .ok_or_else(resource)?;
    budget.charge(spliced.len())?;
    let reparsed =
        decode_with_retained_bytes(Arc::from(spliced.as_slice()), retained, retained_appearance)?;
    verify(scene, source, &reparsed, &[], Appearance::Relocated)?;
    drop(reparsed);
    // Live here: source, output, spliced and an Arc copy of the last two. The 2 * MAX_BYTES check
    // above bounds source + output + spliced, so the total stays under 4 * MAX_BYTES.
    budget.charge(output.len())?;
    budget.charge(spliced.len())?;
    let before = container::inspect(Arc::from(output.as_slice()))?;
    budget.charge(before.metadata_bytes().ok_or_else(resource)?)?;
    let after = container::inspect(Arc::from(spliced.as_slice()))?;
    budget.charge(after.metadata_bytes().ok_or_else(resource)?)?;
    place::check(scene, &before, &after, plan, &mut budget).map_err(
        |violation| match violation {
            place::Violation::Resource => resource(),
            _ => unsupported(),
        },
    )?;
    Ok(spliced)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Appearance {
    /// Same bytes and chunk indices as the original decode: compare the whole metadata.
    Exact,
    /// Spliced bytes shift offsets and chunk indices: compare coordinate-free facts only.
    Relocated,
}

/// Owner-sorted membership; hand-built plans can name more than one field of an owner.
fn has_field(
    fields: &[(NativeId, TransformField)],
    owner: NativeId,
    field: TransformField,
) -> bool {
    let start = fields.partition_point(|entry| entry.0 < owner);
    fields[start..]
        .iter()
        .take_while(|entry| entry.0 == owner)
        .any(|entry| entry.1 == field)
}

/// Compares a reparse with the scene. Fields named in owner-sorted `skip` are still absent in `parsed`.
fn verify(
    scene: &NativeScene,
    source: &crate::native_scene::source::SourceRetention,
    parsed: &NativeScene,
    skip: &[(NativeId, TransformField)],
    appearance: Appearance,
) -> Result<(), BinaryError> {
    if !same_structure(scene, parsed) {
        return Err(unsupported());
    }
    if appearance == Appearance::Exact {
        check_appearance_metadata(scene, parsed)?;
    } else if !same_appearance_shape(
        scene
            .source
            .as_ref()
            .and_then(|source| source.appearance.as_deref()),
        parsed
            .source
            .as_ref()
            .and_then(|source| source.appearance.as_deref()),
    ) {
        return Err(unsupported());
    }
    if scene.materials.len() != parsed.materials.len()
        || scene
            .materials
            .iter()
            .zip(&parsed.materials)
            .any(|(expected, actual)| {
                expected.binding != actual.binding
                    || expected.value.to_bits() != actual.value.to_bits()
            })
    {
        return Err(unsupported());
    }
    for (node, decoded) in scene.nodes.iter().zip(&parsed.nodes) {
        for field in FIELDS {
            if has_field(skip, node.transform_id, field) {
                continue;
            }
            for (component, (value, after)) in node
                .field(field)
                .iter()
                .zip(decoded.field(field))
                .enumerate()
            {
                let expected = value
                    .internal(
                        Some(source),
                        &scene.units,
                        node.transform_id,
                        field,
                        component,
                    )
                    .map_err(|e| canonical(e, None))?;
                let actual = after
                    .internal(
                        parsed.source.as_ref(),
                        &parsed.units,
                        decoded.transform_id,
                        field,
                        component,
                    )
                    .map_err(|e| canonical(e, None))?;
                if expected.to_bits() != actual.to_bits() {
                    return Err(unsupported());
                }
            }
        }
    }
    Ok(())
}

mod insert;
mod place;
mod plan;
#[cfg(test)]
mod tests;
