//! Checked splice of whole chunks into the source bytes (PROVISIONAL, issue #19).
//!
//! IFF has no absolute offsets, so inserting a chunk only needs the size word of every enclosing
//! group to grow by its length. The caller supplies the chunk bytes and where they go.

use super::{BinaryError, Budget, resource, unsupported};
use crate::maya_binary::container::MAX_BYTES;
use crate::native_scene::{NativeId, source::TransformField};

const SIZE_WORD: std::ops::Range<usize> = 8..16;
const HEADER_BYTES: usize = 16;

/// A chunk to place at `at`, in coordinates of the bytes before the splice.
///
/// `owner` and `field` name the transform leaf it adds. `ancestors` are the header starts of the
/// groups that must grow with it.
#[derive(Debug)]
pub(super) struct Insertion {
    pub(super) owner: NativeId,
    pub(super) field: TransformField,
    pub(super) at: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) ancestors: Vec<usize>,
}

/// Bytes inserted before each plan index: `shift[k]` is the total length of the first `k` records.
fn shifts(plan: &[Insertion], budget: &mut Budget) -> Result<Vec<usize>, BinaryError> {
    let mut shift = budget.vector::<usize>(plan.len() + 1)?;
    shift.push(0);
    for insertion in plan {
        let total = shift[shift.len() - 1]
            .checked_add(insertion.bytes.len())
            .ok_or_else(resource)?;
        shift.push(total);
    }
    Ok(shift)
}

/// `(ancestor header start, total growth)` in original coordinates, ascending, one per ancestor.
fn growth(plan: &[Insertion], budget: &mut Budget) -> Result<Vec<(usize, usize)>, BinaryError> {
    let pairs = plan.iter().try_fold(0usize, |sum, insertion| {
        sum.checked_add(insertion.ancestors.len())
    });
    let mut all = budget.vector::<(usize, usize)>(pairs.ok_or_else(resource)?)?;
    for insertion in plan {
        all.extend(
            insertion
                .ancestors
                .iter()
                .map(|start| (*start, insertion.bytes.len())),
        );
    }
    all.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, len) in all {
        match merged.last_mut() {
            Some((last, total)) if *last == start => {
                *total = total.checked_add(len).ok_or_else(resource)?;
            }
            _ => merged.push((start, len)),
        }
    }
    Ok(merged)
}

fn check_plan(patched: &[u8], plan: &[Insertion]) -> Result<(), BinaryError> {
    if plan.windows(2).any(|pair| pair[0].at >= pair[1].at)
        || plan.iter().any(|insertion| {
            insertion.at > patched.len()
                || insertion
                    .ancestors
                    .iter()
                    .any(|start| *start >= insertion.at)
        })
    {
        return Err(unsupported());
    }
    Ok(())
}

/// Returns `patched` with each insertion placed and every ancestor size word grown.
///
/// Insertions must be strictly ascending by `at`. The result is at most `MAX_BYTES` and is charged
/// to `budget`. This proves the copy and size-word arithmetic only; it does not know whether `at`
/// or the ancestors name the right group, which the caller's reparse and placement check decide.
pub(super) fn splice(
    patched: &[u8],
    plan: &[Insertion],
    budget: &mut Budget,
) -> Result<Vec<u8>, BinaryError> {
    check_plan(patched, plan)?;
    let shift = shifts(plan, budget)?;
    let total = patched
        .len()
        .checked_add(shift[plan.len()])
        .filter(|total| *total <= MAX_BYTES)
        .ok_or_else(resource)?;
    let mut output = budget.vector::<u8>(total)?;
    let mut cursor = 0;
    for insertion in plan {
        output.extend_from_slice(&patched[cursor..insertion.at]);
        output.extend_from_slice(&insertion.bytes);
        cursor = insertion.at;
    }
    output.extend_from_slice(&patched[cursor..]);
    for (start, delta) in growth(plan, budget)? {
        let moved = start
            .checked_add(shift[plan.partition_point(|insertion| insertion.at <= start)])
            .ok_or_else(resource)?;
        resize_word(&mut output, moved, |size| {
            u64::try_from(delta)
                .ok()
                .and_then(|delta| size.checked_add(delta))
        })?;
    }
    Ok(output)
}

/// Arithmetic self-check of [`splice`]: removing the inserted ranges and shrinking the ancestor size
/// words must give back `patched` byte for byte, and each removed range must be the planned bytes.
pub(super) fn check_inverse(
    spliced: &[u8],
    patched: &[u8],
    plan: &[Insertion],
    budget: &mut Budget,
) -> Result<(), BinaryError> {
    check_plan(patched, plan)?;
    let mut restored = budget.vector::<u8>(patched.len())?;
    let (mut old, mut new) = (0usize, 0usize);
    for insertion in plan {
        let kept = insertion.at.checked_sub(old).ok_or_else(unsupported)?;
        let inserted = new.checked_add(kept).ok_or_else(resource)?;
        let end = inserted
            .checked_add(insertion.bytes.len())
            .ok_or_else(resource)?;
        if spliced.get(inserted..end) != Some(insertion.bytes.as_slice()) {
            return Err(unsupported());
        }
        restored.extend_from_slice(spliced.get(new..inserted).ok_or_else(unsupported)?);
        (old, new) = (insertion.at, end);
    }
    restored.extend_from_slice(spliced.get(new..).ok_or_else(unsupported)?);
    for (start, delta) in growth(plan, budget)? {
        resize_word(&mut restored, start, |size| {
            u64::try_from(delta)
                .ok()
                .and_then(|delta| size.checked_sub(delta))
        })?;
    }
    if restored != patched {
        return Err(unsupported());
    }
    Ok(())
}

fn resize_word(
    output: &mut [u8],
    header: usize,
    resize: impl FnOnce(u64) -> Option<u64>,
) -> Result<(), BinaryError> {
    let end = header.checked_add(HEADER_BYTES).ok_or_else(resource)?;
    let header = output.get_mut(header..end).ok_or_else(unsupported)?;
    if !matches!(&header[..4], b"FOR8" | b"LIS8") {
        return Err(unsupported());
    }
    let word = &mut header[SIZE_WORD];
    let size = u64::from_be_bytes((&*word).try_into().map_err(|_| unsupported())?);
    word.copy_from_slice(&resize(size).ok_or_else(resource)?.to_be_bytes());
    Ok(())
}

#[cfg(test)]
mod tests;
