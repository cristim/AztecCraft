//! Reference analysis for dropping a root transform and its exclusive shapes from an opened
//! Maya Binary (PROVISIONAL, issue #272).
//!
//! Records name nodes by string, never by index, so a removal is safe only when no surviving
//! record names a removed node. Anything this analysis cannot prove is refused.

use super::binding::is_bound;
use crate::{
    maya_binary::{
        container::{self},
        semantic::{self, Data, WireFacts},
    },
    native_scene::NativeId,
};
use std::{fmt, ops::Range, sync::Arc};

/// Why a sourced Maya Binary node cannot be removed yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalRefusal {
    /// The retained bytes no longer decode or lay out as expected.
    Unreadable,
    /// Another node names the transform as its parent.
    ForeignChild,
    /// A connection, relation or selection names a removed node in a way that is not droppable.
    NamedElsewhere,
}

impl fmt::Display for RemovalRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unreadable => "The opened file's records could not be re-read for deletion",
            Self::ForeignChild => "Another node is parented under this one",
            Self::NamedElsewhere => "The file refers to this node by name from another record",
        })
    }
}

fn plain(name: &[u8]) -> &[u8] {
    name.strip_prefix(b":").unwrap_or(name)
}

/// The node part of an `owner.attribute` endpoint or of a bare node name, as written.
fn node_part(endpoint: &[u8]) -> &[u8] {
    endpoint
        .split(|byte| *byte == b'.')
        .next()
        .unwrap_or(endpoint)
}

/// Whether any `|`- or `:`-separated component of the node part is in `names` (`|a|as` names `a`).
fn names_path(names: &[&[u8]], endpoint: &[u8]) -> bool {
    node_part(endpoint)
        .split(|byte| matches!(byte, b'|' | b':'))
        .any(|component| names.contains(&component))
}

fn form(facts: &WireFacts<'_>, index: usize) -> Option<[u8; 4]> {
    facts
        .container
        .chunk(facts.records[index].group)
        .and_then(|chunk| chunk.form())
}

fn text<'a>(facts: &'a WireFacts<'_>, span: &Range<usize>) -> &'a [u8] {
    &facts.container.source()[span.clone()]
}

/// Checks that `transform` and its exclusive `shapes` can be dropped from `bytes` without a
/// surviving record naming them. Ids are the opener's binding order (`NativeId` = index + 1).
pub(in crate::native_scene) fn check_removal(
    bytes: &Arc<[u8]>,
    transform: NativeId,
    shapes: &[NativeId],
) -> Result<(), RemovalRefusal> {
    let container = container::inspect(bytes.clone()).map_err(|_| RemovalRefusal::Unreadable)?;
    let facts = semantic::decode_records(&container).map_err(|_| RemovalRefusal::Unreadable)?;
    let bound: Vec<usize> = (0..facts.records.len())
        .filter(|index| is_bound(&container, &facts.records[*index]))
        .collect();
    let record_of = |id: NativeId| {
        usize::try_from(id.0)
            .ok()
            .and_then(|id| id.checked_sub(1))
            .and_then(|id| bound.get(id).copied())
            .ok_or(RemovalRefusal::Unreadable)
    };
    let root = record_of(transform)?;
    let mut removed = vec![root];
    for shape in shapes {
        removed.push(record_of(*shape)?);
    }
    if form(&facts, root) != Some(*b"XFRM")
        || removed[1..]
            .iter()
            .any(|index| form(&facts, *index) != Some(*b"DMSH"))
    {
        return Err(RemovalRefusal::Unreadable);
    }
    let mut names: Vec<&[u8]> = Vec::with_capacity(removed.len());
    for index in &removed {
        let Data::Create(create) = &facts.records[*index].data else {
            return Err(RemovalRefusal::Unreadable);
        };
        names.push(plain(text(&facts, &create.name)));
    }
    let named = |span: &Range<usize>| names_path(&names, text(&facts, span));
    let parents: Vec<Option<&[u8]>> = facts
        .records
        .iter()
        .map(|record| match &record.data {
            Data::Create(create) => create.parent.as_ref().map(|span| text(&facts, span)),
            _ => None,
        })
        .collect();
    if removed[1..]
        .iter()
        .any(|index| parents[*index].map(plain) != Some(names[0]))
    {
        return Err(RemovalRefusal::ForeignChild);
    }
    if parents.iter().enumerate().any(|(index, parent)| {
        !removed.contains(&index) && parent.is_some_and(|parent| names_path(&names, parent))
    }) {
        return Err(RemovalRefusal::ForeignChild);
    }
    for record in &facts.records {
        match &record.data {
            Data::Selection(target) if named(target) => {
                return Err(RemovalRefusal::NamedElsewhere);
            }
            Data::Relation(relation)
                if [&relation.head, &relation.tails[0], &relation.tails[1]]
                    .into_iter()
                    .any(named) =>
            {
                return Err(RemovalRefusal::NamedElsewhere);
            }
            Data::Connection(connection)
                if named(&connection.source) || named(&connection.destination) =>
            {
                return Err(RemovalRefusal::NamedElsewhere);
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
