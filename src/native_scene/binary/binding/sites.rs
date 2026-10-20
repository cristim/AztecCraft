//! Insertion sites for an absent default `t` leaf (PROVISIONAL, issue #19).
//!
//! The walk is over raw container children, because `decode_records` hides tags it does not
//! recognise. Grammar: `.project-docs/binary-leaf-insertion.md`.

use super::{Binding, children, effects::key, field};
use crate::{
    maya_binary::container::Container,
    native_scene::{
        binary::{BinaryError, admission::Budget},
        source::{InsertSite, TransformField},
    },
};

const FIRST_RANK_MAX: usize = 2;

/// Byte offset of the new `t` record when `binding` matches the grammar, else `None`.
///
/// Decoding already guarantees what is not re-checked here: the group is `FOR8` `XFRM`, its first
/// child is the `CREA`, a `DBLE` is the `v` visibility word (kind `0x24`) unless a custom attribute
/// precedes it, and a `DBL3` key is one of t/r/s/rp/sp (`effects::inspect` rejects anything else).
fn place(container: &Container, binding: &Binding) -> Option<usize> {
    binding.transform?;
    let mut kids = children(container, binding.group);
    kids.next()?;
    let mut seen_visibility = false;
    let mut last_rank: Option<usize> = None;
    let mut first_triple = None;
    for chunk in kids {
        match &chunk.tag() {
            b"DBLE" if !seen_visibility && last_rank.is_none() => seen_visibility = true,
            b"DBL3" => {
                let rank = field(key(container, chunk).ok()?)?;
                let ordered = match last_rank {
                    Some(last) => rank > last,
                    // Rank 0 is `t` itself, so an existing translation leaf is refused here and below.
                    None => (1..=FIRST_RANK_MAX).contains(&rank),
                };
                if !ordered {
                    return None;
                }
                last_rank = Some(rank);
                first_triple.get_or_insert(chunk.header_range().start);
            }
            _ => return None,
        }
    }
    Some(first_triple.unwrap_or(container.chunk(binding.group)?.payload_range().end))
}

pub(super) fn collect(
    container: &Container,
    bindings: &[Binding],
    budget: &mut Budget,
) -> Result<Vec<InsertSite>, BinaryError> {
    let mut sites = budget.vector(bindings.len())?;
    for binding in bindings {
        let Some(at) = place(container, binding) else {
            continue;
        };
        let mut chain = Vec::new();
        let mut next = Some(binding.group);
        while let Some(index) = next {
            let chunk = container.chunk(index).ok_or_else(super::resource)?;
            chain.push(chunk.header_range().start);
            next = chunk.parent();
        }
        let mut ancestors = budget.vector(chain.len())?;
        ancestors.extend(chain);
        sites.push(InsertSite {
            owner: binding.id,
            field: TransformField::Translation,
            at,
            ancestors,
        });
    }
    Ok(sites)
}
