use super::{
    BinaryError, ErrorKind, admission::Budget, canonical, decode_with_retained_bytes, resource,
};
use crate::native_scene::{NativeScene, NativeTransformNode, source::SourceFormat};
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
    let mut output = budget.vector::<u8>(source.bytes.len())?;
    output.extend_from_slice(&source.bytes);
    for node in &scene.nodes {
        for field in FIELDS {
            for (component, scalar) in node.field(field).iter().enumerate() {
                if !scalar.replaced {
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
    if !same_structure(scene, &parsed) {
        return Err(unsupported());
    }
    check_appearance_metadata(scene, &parsed)?;
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
    Ok(output)
}

#[cfg(test)]
mod insert;
#[cfg(test)]
mod tests;
