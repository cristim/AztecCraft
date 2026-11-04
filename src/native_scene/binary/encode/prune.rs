//! Save of a scene whose removed nodes' records are dropped from the source bytes (PROVISIONAL,
//! issue #272). The word patches are already in `output`; this removes the records, proves the
//! arithmetic by inverse, reopens the result and compares it with the scene through the id remap.

use super::{
    Appearance,
    insert::{Edit, check_inverse_edits, splice_edits},
    unsupported, verify,
};
use crate::native_scene::{
    NativeScene,
    binary::{
        BinaryError,
        admission::Budget,
        decode_with_retained_bytes,
        removal::assert_pruned,
        removal::{check_dropped, check_removal},
        resource,
    },
    source::SourceRetention,
    source_removal::IdRemap,
};
use std::sync::Arc;

pub(super) fn prune(
    scene: &NativeScene,
    source: &SourceRetention,
    output: &[u8],
    remap: &IdRemap,
    names: &[String],
    (retained, retained_appearance): (usize, usize),
    budget: &mut Budget,
) -> Result<Vec<u8>, BinaryError> {
    let ranges = check_removal(&source.bytes, &scene.removed).map_err(|_| unsupported())?;
    let mut edits = budget.vector::<Edit>(ranges.len())?;
    edits.extend(ranges.iter().map(|prune| Edit {
        at: prune.range.start,
        remove_len: prune.range.len(),
        bytes: &[],
        ancestors: &prune.ancestors,
    }));
    let pruned = splice_edits(output, &edits, budget)?;
    check_inverse_edits(&pruned, output, &edits, budget)?;
    let retained = retained
        .checked_add(pruned.capacity())
        .filter(|retained| *retained <= 2 * crate::maya_binary::container::MAX_BYTES)
        .ok_or_else(resource)?;
    budget.charge(pruned.len())?;
    let reopened =
        decode_with_retained_bytes(Arc::from(pruned.as_slice()), retained, retained_appearance)?;
    let expected = scene.renumbered(remap);
    verify(
        scene,
        &expected,
        source,
        &reopened,
        &[],
        Appearance::Relocated,
    )?;
    drop(reopened);
    budget.charge(pruned.len())?;
    let names: Vec<&[u8]> = names.iter().map(String::as_bytes).collect();
    let written: Arc<[u8]> = Arc::from(pruned.as_slice());
    check_dropped(&source.bytes, &scene.removed, &ranges, &written).map_err(|_| unsupported())?;
    assert_pruned(&written, &names).map_err(|_| unsupported())?;
    Ok(pruned)
}

#[cfg(test)]
mod tests;
