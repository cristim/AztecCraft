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

/// One splice in original coordinates: `remove_len` bytes at `at` are replaced by `bytes`.
///
/// An insertion has `remove_len == 0`; a prune has no bytes. `ancestors` are the header starts of
/// the groups whose size word changes by `bytes.len() - remove_len`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Edit<'a> {
    pub(super) at: usize,
    pub(super) remove_len: usize,
    pub(super) bytes: &'a [u8],
    pub(super) ancestors: &'a [usize],
}

impl Edit<'_> {
    fn delta(&self) -> Result<isize, BinaryError> {
        let added = isize::try_from(self.bytes.len()).map_err(|_| resource())?;
        let removed = isize::try_from(self.remove_len).map_err(|_| resource())?;
        added.checked_sub(removed).ok_or_else(resource)
    }

    fn end(&self) -> Option<usize> {
        self.at.checked_add(self.remove_len)
    }
}

impl Insertion {
    fn edit(&self) -> Edit<'_> {
        Edit {
            at: self.at,
            remove_len: 0,
            bytes: &self.bytes,
            ancestors: &self.ancestors,
        }
    }
}

fn edits<'a>(plan: &'a [Insertion], budget: &mut Budget) -> Result<Vec<Edit<'a>>, BinaryError> {
    let mut all = budget.vector::<Edit<'a>>(plan.len())?;
    all.extend(plan.iter().map(Insertion::edit));
    Ok(all)
}

/// Signed bytes added before each plan index: `shift[k]` is the net delta of the first `k` edits.
fn shifts(plan: &[Edit], budget: &mut Budget) -> Result<Vec<isize>, BinaryError> {
    let mut shift = budget.vector::<isize>(plan.len() + 1)?;
    shift.push(0);
    for edit in plan {
        let total = shift[shift.len() - 1]
            .checked_add(edit.delta()?)
            .ok_or_else(resource)?;
        shift.push(total);
    }
    Ok(shift)
}

/// `(ancestor header start, total signed delta)` in original coordinates, ascending, one per
/// ancestor.
fn growth(plan: &[Edit], budget: &mut Budget) -> Result<Vec<(usize, isize)>, BinaryError> {
    let pairs = plan
        .iter()
        .try_fold(0usize, |sum, edit| sum.checked_add(edit.ancestors.len()));
    let mut all = budget.vector::<(usize, isize)>(pairs.ok_or_else(resource)?)?;
    for edit in plan {
        let delta = edit.delta()?;
        all.extend(edit.ancestors.iter().map(|start| (*start, delta)));
    }
    all.sort_unstable();
    let mut merged: Vec<(usize, isize)> = Vec::new();
    for (start, delta) in all {
        match merged.last_mut() {
            Some((last, total)) if *last == start => {
                *total = total.checked_add(delta).ok_or_else(resource)?;
            }
            _ => merged.push((start, delta)),
        }
    }
    Ok(merged)
}

/// Edits must be ascending and disjoint; two insertions at one point are refused.
fn check_plan(patched: &[u8], plan: &[Edit]) -> Result<(), BinaryError> {
    let ordered = plan.windows(2).all(|pair| {
        pair[0].end().is_some_and(|end| end <= pair[1].at)
            && (pair[0].remove_len > 0 || pair[0].at < pair[1].at)
    });
    let inside = plan.iter().all(|edit| {
        edit.end().is_some_and(|end| end <= patched.len())
            && edit.ancestors.iter().all(|start| *start < edit.at)
    });
    if !ordered || !inside {
        return Err(unsupported());
    }
    Ok(())
}

/// Where the ancestor header at original `start` sits after the edits before it.
fn moved(plan: &[Edit], shift: &[isize], start: usize) -> Result<usize, BinaryError> {
    let before = plan.partition_point(|edit| edit.at <= start);
    if plan[..before]
        .iter()
        .any(|edit| edit.end() > Some(start) && edit.remove_len > 0)
    {
        return Err(unsupported());
    }
    start.checked_add_signed(shift[before]).ok_or_else(resource)
}

fn grow(size: u64, delta: isize) -> Option<u64> {
    size.checked_add_signed(i64::try_from(delta).ok()?)
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
    let plan = edits(plan, budget)?;
    splice_edits(patched, &plan, budget)
}

/// [`splice`] for signed edits: removed ranges are dropped and the ancestor size words move by the
/// net delta of the edits that name them.
pub(super) fn splice_edits(
    patched: &[u8],
    plan: &[Edit],
    budget: &mut Budget,
) -> Result<Vec<u8>, BinaryError> {
    check_plan(patched, plan)?;
    let shift = shifts(plan, budget)?;
    let total = patched
        .len()
        .checked_add_signed(shift[plan.len()])
        .filter(|total| *total <= MAX_BYTES)
        .ok_or_else(resource)?;
    let mut output = budget.vector::<u8>(total)?;
    let mut cursor = 0;
    for edit in plan {
        output.extend_from_slice(&patched[cursor..edit.at]);
        output.extend_from_slice(edit.bytes);
        cursor = edit.end().ok_or_else(resource)?;
    }
    output.extend_from_slice(&patched[cursor..]);
    for (start, delta) in growth(plan, budget)? {
        let at = moved(plan, &shift, start)?;
        resize_word(&mut output, at, |size| grow(size, delta))?;
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
    let plan = edits(plan, budget)?;
    check_inverse_edits(spliced, patched, &plan, budget)
}

/// [`check_inverse`] for signed edits. The bytes a prune removed are not in `spliced`, so they are
/// taken from `patched`: the proof covers everything outside the removed ranges and every size word.
pub(super) fn check_inverse_edits(
    spliced: &[u8],
    patched: &[u8],
    plan: &[Edit],
    budget: &mut Budget,
) -> Result<(), BinaryError> {
    check_plan(patched, plan)?;
    let mut restored = budget.vector::<u8>(patched.len())?;
    let (mut old, mut new) = (0usize, 0usize);
    for edit in plan {
        let kept = edit.at.checked_sub(old).ok_or_else(unsupported)?;
        let inserted = new.checked_add(kept).ok_or_else(resource)?;
        let end = inserted
            .checked_add(edit.bytes.len())
            .ok_or_else(resource)?;
        if spliced.get(inserted..end) != Some(edit.bytes) {
            return Err(unsupported());
        }
        restored.extend_from_slice(spliced.get(new..inserted).ok_or_else(unsupported)?);
        restored.extend_from_slice(&patched[edit.at..edit.end().ok_or_else(resource)?]);
        (old, new) = (edit.end().ok_or_else(resource)?, end);
    }
    restored.extend_from_slice(spliced.get(new..).ok_or_else(unsupported)?);
    for (start, delta) in growth(plan, budget)? {
        let inverse = delta.checked_neg().ok_or_else(resource)?;
        resize_word(&mut restored, start, |size| grow(size, inverse))?;
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
