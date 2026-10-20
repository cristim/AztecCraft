//! Checked splice of whole chunks into the source bytes (PROVISIONAL, issue #19).
//!
//! IFF has no absolute offsets, so inserting a chunk only needs the size word of every enclosing
//! group to grow by its length. The caller supplies the chunk bytes and where they go.

use super::{BinaryError, Budget, resource, unsupported};
use crate::maya_binary::container::MAX_BYTES;

const SIZE_WORD: std::ops::Range<usize> = 8..16;
const HEADER_BYTES: usize = 16;

/// A chunk to place at `at`, in coordinates of the bytes before the splice.
///
/// `ancestors` are the header starts of the groups that must grow with it.
pub(super) struct Insertion {
    pub(super) at: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) ancestors: Vec<usize>,
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
    let mut shift = budget.vector::<usize>(plan.len() + 1)?;
    shift.push(0);
    for insertion in plan {
        let total = shift[shift.len() - 1]
            .checked_add(insertion.bytes.len())
            .ok_or_else(resource)?;
        shift.push(total);
    }
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

    let pairs = plan.iter().try_fold(0usize, |sum, insertion| {
        sum.checked_add(insertion.ancestors.len())
    });
    let mut growth = budget.vector::<(usize, usize)>(pairs.ok_or_else(resource)?)?;
    for insertion in plan {
        growth.extend(
            insertion
                .ancestors
                .iter()
                .map(|start| (*start, insertion.bytes.len())),
        );
    }
    growth.sort_unstable();
    let mut index = 0;
    while index < growth.len() {
        let start = growth[index].0;
        let mut delta = 0usize;
        while let Some((_, len)) = growth.get(index).filter(|(at, _)| *at == start) {
            delta = delta.checked_add(*len).ok_or_else(resource)?;
            index += 1;
        }
        let moved = start + shift[plan.partition_point(|insertion| insertion.at <= start)];
        grow(&mut output, moved, delta)?;
    }
    Ok(output)
}

fn grow(output: &mut [u8], header: usize, delta: usize) -> Result<(), BinaryError> {
    let end = header.checked_add(HEADER_BYTES).ok_or_else(resource)?;
    let header = output.get_mut(header..end).ok_or_else(unsupported)?;
    if !matches!(&header[..4], b"FOR8" | b"LIS8") {
        return Err(unsupported());
    }
    let word = &mut header[SIZE_WORD];
    let size = u64::from_be_bytes((&*word).try_into().map_err(|_| unsupported())?);
    let grown = u64::try_from(delta)
        .ok()
        .and_then(|delta| size.checked_add(delta))
        .ok_or_else(resource)?;
    word.copy_from_slice(&grown.to_be_bytes());
    Ok(())
}

#[cfg(test)]
mod tests;
