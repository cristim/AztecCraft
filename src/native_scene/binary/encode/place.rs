//! Placement assertion for spliced bytes (PROVISIONAL, issue #19).
//!
//! `check_inverse` only proves the copy and size-word arithmetic, so it cannot tell a wrong offset
//! or a wrong ancestor. This compares the container trees of the patched copy and the spliced bytes
//! and derives everything from structure: where each new chunk sits, which group is its parent
//! (found through the owner's creation record in the old tree), how its siblings are ordered and
//! which chunks may differ. Nothing is read from `InsertSite`.

use super::{Budget, FIELDS, Insertion};
use crate::native_scene::binary::binding::children;
use crate::{
    maya_binary::container::{Chunk, Container},
    native_scene::{NativeId, NativeScene, source::TransformField},
};

/// The rule a spliced file broke.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Violation {
    Resource,
    /// The chunk counts differ by something other than the planned records.
    Count,
    /// No chunk starts where the plan says a record went.
    Position,
    /// The new chunk is not the planned `DBL3` with the file's aux word.
    Record,
    /// The owner has no `XFRM` creation record in the patched tree.
    Owner,
    /// The new chunk's parent is not the owner's group.
    Parent,
    /// The parent's children are not `CREA`, optional `v`, then strictly ascending triples.
    Order,
    /// An existing chunk moved, changed, or has a size word that differs by more than the records.
    Chunk,
    /// The total length did not grow by exactly the planned bytes.
    Length,
}

fn key(field: TransformField) -> &'static [u8] {
    match field {
        TransformField::Translation => b"t",
        TransformField::Rotation => b"r",
        TransformField::Scale => b"s",
        TransformField::RotatePivot => b"rp",
        TransformField::ScalePivot => b"sp",
    }
}

fn rank(name: &[u8]) -> Option<usize> {
    FIELDS.iter().position(|field| key(*field) == name)
}

fn payload<'a>(container: &'a Container, chunk: &Chunk) -> &'a [u8] {
    &container.source()[chunk.payload_range()]
}

/// The key of a `DBL3`/`DBLE` payload: the bytes before the first NUL.
fn key_of<'a>(container: &'a Container, chunk: &Chunk) -> Option<&'a [u8]> {
    let bytes = payload(container, chunk);
    Some(&bytes[..bytes.iter().position(|b| *b == 0)?])
}

/// Children of `parent` must read `CREA`, at most one `v`, then triples with ascending ranks.
fn ordered(container: &Container, parent: usize) -> bool {
    let mut kids = children(container, parent);
    if kids.next().is_none_or(|first| first.tag() != *b"CREA") {
        return false;
    }
    let mut visibility = true;
    let mut last = None;
    for chunk in kids {
        let name = key_of(container, chunk);
        match &chunk.tag() {
            b"DBLE" if visibility && last.is_none() && name == Some(b"v") => visibility = false,
            b"DBL3" => {
                let Some(next) = name.and_then(rank) else {
                    return false;
                };
                if last.is_some_and(|last| next <= last) {
                    return false;
                }
                last = Some(next);
            }
            _ => return false,
        }
    }
    true
}

pub(super) fn check(
    scene: &NativeScene,
    old: &Container,
    new: &Container,
    plan: &[Insertion],
    budget: &mut Budget,
) -> Result<(), Violation> {
    let (before, after) = (old.chunks(), new.chunks());
    let added: usize = plan.iter().map(|i| i.bytes.len()).sum();
    if after.len() != before.len() + plan.len() {
        return Err(Violation::Count);
    }
    if new.source().len() != old.source().len() + added {
        return Err(Violation::Length);
    }
    // Index of the first old chunk at or after each record's position, ascending with the plan.
    let mut slots = budget
        .vector::<usize>(plan.len())
        .map_err(|_| Violation::Resource)?;
    slots.extend(plan.iter().map(|insertion| {
        before.partition_point(|chunk| chunk.header_range().start < insertion.at)
    }));
    let moved = |index: usize| index + slots.partition_point(|slot| *slot <= index);
    let mut grown = budget
        .vector::<usize>(after.len())
        .map_err(|_| Violation::Resource)?;
    grown.resize(after.len(), 0);
    let aux = before
        .iter()
        .find(|chunk| chunk.tag() == *b"DBL3")
        .map(Chunk::auxiliary);
    // Resolve creation records once by name, then index their groups by owner.
    let mut owners = budget
        .vector::<(&[u8], NativeId, Option<usize>)>(scene.nodes.len())
        .map_err(|_| Violation::Resource)?;
    owners.extend(
        scene
            .nodes
            .iter()
            .map(|node| (node.name.as_bytes(), node.transform_id, None)),
    );
    owners.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for chunk in before.iter().filter(|chunk| chunk.tag() == *b"CREA") {
        let Some(group) = chunk.parent().filter(|group| {
            old.chunk(*group)
                .is_some_and(|chunk| chunk.form() == Some(*b"XFRM"))
        }) else {
            continue;
        };
        let Some(text) = payload(old, chunk).get(1..) else {
            continue;
        };
        let Some(end) = text.iter().position(|byte| *byte == 0) else {
            continue;
        };
        let name = text[..end].strip_prefix(b":").unwrap_or(&text[..end]);
        if let Ok(index) = owners.binary_search_by_key(&name, |entry| entry.0) {
            owners[index].2.get_or_insert(group);
        }
    }
    owners.sort_unstable_by_key(|entry| entry.1);
    let mut shift = 0;
    for (j, insertion) in plan.iter().enumerate() {
        let k = slots[j] + j;
        let chunk = after.get(k).ok_or(Violation::Position)?;
        if chunk.header_range().start != insertion.at + shift {
            return Err(Violation::Position);
        }
        shift += insertion.bytes.len();
        let name = key(insertion.field);
        let bytes = payload(new, chunk);
        if chunk.tag() != *b"DBL3"
            || Some(chunk.auxiliary()) != aux
            || bytes.len() != name.len() + 2 + 24
            || !bytes.starts_with(name)
            || bytes[name.len()..name.len() + 2] != [0, 0x20]
            || new.source()[chunk.padding_range()].iter().any(|b| *b != 0)
        {
            return Err(Violation::Record);
        }
        let group = owners
            .binary_search_by_key(&insertion.owner, |entry| entry.1)
            .ok()
            .and_then(|index| owners[index].2)
            .ok_or(Violation::Owner)?;
        if chunk.parent() != Some(moved(group)) {
            return Err(Violation::Parent);
        }
        if !ordered(new, moved(group)) {
            return Err(Violation::Order);
        }
        let mut up = chunk.parent();
        while let Some(index) = up {
            grown[index] += insertion.bytes.len();
            up = after[index].parent();
        }
    }
    for (index, old_chunk) in before.iter().enumerate() {
        let now = moved(index);
        let new_chunk = after.get(now).ok_or(Violation::Chunk)?;
        let same = new_chunk.tag() == old_chunk.tag()
            && new_chunk.form() == old_chunk.form()
            && new_chunk.auxiliary() == old_chunk.auxiliary()
            && new_chunk.parent() == old_chunk.parent().map(moved)
            && new_chunk.payload_range().len() == old_chunk.payload_range().len() + grown[now]
            && new.source()[new_chunk.padding_range()] == old.source()[old_chunk.padding_range()]
            && (old_chunk.form().is_some() || payload(new, new_chunk) == payload(old, old_chunk));
        if !same {
            return Err(Violation::Chunk);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
