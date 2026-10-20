//! Chooses which absent default leaves a save must insert (PROVISIONAL, issue #19).
//!
//! An absent leaf is a field whose three scalars have no source origin. A bit-changed scalar is
//! marked `replaced`, so a field with any replaced component and no origin is an edit that needs a
//! new record. Grammar and limits: `.project-docs/binary-leaf-insertion.md`.

use super::{BinaryError, FIELDS, canonical, insert::Insertion, resource, unsupported};
use crate::native_scene::{
    NativeScene,
    binary::write::chunk::{Align, leaf},
    source::{SourceRetention, TransformField},
};

/// Fields whose absent leaf may be inserted. Empty until the edit paths admit them.
pub(super) const INSERTABLE: &[TransformField] = &[];

/// The record for `field` holding `words` (internal units), with the file's own `aux`.
///
/// Only translation has a reviewed shape: payload `t NUL 0x20` plus three big-endian f64, padded
/// with zeros to 8 bytes by the shared chunk builder.
pub(super) fn build_record(
    field: TransformField,
    aux: u32,
    words: [f64; 3],
) -> Result<Vec<u8>, BinaryError> {
    if field != TransformField::Translation {
        return Err(unsupported());
    }
    let mut payload = b"t\0\x20".to_vec();
    for word in words {
        payload.extend_from_slice(&word.to_be_bytes());
    }
    leaf(*b"DBL3", aux, &payload, Align::Eight).map_err(|_| resource())
}

/// Insertions for every edited absent leaf of a field in `insertable`, in node order, which is file order; `splice` refuses anything else.
pub(super) fn plan(
    scene: &NativeScene,
    source: &SourceRetention,
    insertable: &[TransformField],
) -> Result<Vec<Insertion>, BinaryError> {
    let mut plan = Vec::new();
    for node in &scene.nodes {
        for field in FIELDS
            .into_iter()
            .filter(|field| insertable.contains(field))
        {
            let scalars = node.field(field);
            let absent = scalars.iter().filter(|s| s.origin.is_none()).count();
            if absent == 0 || !scalars.iter().any(|s| s.replaced) {
                continue;
            }
            if absent != scalars.len() {
                return Err(unsupported());
            }
            let mut words = [0.0; 3];
            for (component, (word, scalar)) in words.iter_mut().zip(scalars).enumerate() {
                if scalar.replaced {
                    *word = scalar
                        .internal(
                            Some(source),
                            &scene.units,
                            node.transform_id,
                            field,
                            component,
                        )
                        .map_err(|e| canonical(e, None))?;
                }
            }
            if words.iter().all(|word| word.to_bits() == 0.0f64.to_bits()) {
                continue;
            }
            let site = source
                .insert_site(node.transform_id, field)
                .ok_or_else(unsupported)?;
            plan.push(Insertion {
                owner: node.transform_id,
                field,
                at: site.at,
                bytes: build_record(field, site.aux, words)?,
                ancestors: site.ancestors.clone(),
            });
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests;
