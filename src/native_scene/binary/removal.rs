//! Reference analysis for dropping a root transform and its exclusive shapes from an opened
//! Maya Binary (PROVISIONAL, issue #272).
//!
//! Records name nodes by string, never by index, so a removal is safe only when every surviving
//! record is shown not to name a removed node. Anything this analysis cannot prove is refused.
//! The refusals and limits are listed in `known-issues.md`, "Delete on an opened Maya Binary".

use super::binding::{effects::normalized, is_bound};
use crate::{
    maya_binary::{
        container::{self, Container},
        semantic::{self, Data, WireFacts},
    },
    native_scene::{NativeId, appearance::*},
};
use std::{collections::HashMap, fmt, ops::Range, sync::Arc};

/// Why a sourced Maya Binary node cannot be removed yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalRefusal {
    /// The retained bytes no longer decode or lay out as expected.
    Unreadable,
    /// A node other than the shapes being removed names the transform as its parent.
    ForeignChild,
    /// A removed shape carries per-face group id or group colour links.
    PerFaceShape,
    /// A removed shape's set membership uses a next-available index, which load would renumber.
    NextAvailableIndex,
    /// A connection, relation or selection names a removed node in a way that is not droppable.
    NamedElsewhere,
    /// A script or text value that stays in the file mentions a removed node.
    TextReference,
    /// An unrecognised leaf of the connection list mentions a removed node.
    UnrecognisedConnection,
    /// The file's text is too large to check for such a mention within the work limit.
    TextScanLimit,
    /// The prune would drop a record that does not belong to the removed nodes.
    UnexpectedDrop,
    /// Removing the records would change the padding of an enclosing group.
    Alignment,
    /// The shape is the only user of a diffuse-bound shader.
    LastDiffuseUser,
    /// A surviving edit needs a record inserted, which cannot be combined with a removal.
    PendingInsertion,
    /// The shape is the only user of a material.
    LastMaterialUser,
    /// Removing the shape would renumber the materials of the shapes that remain.
    FirstMaterialUser,
}

impl fmt::Display for RemovalRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unreadable => "The opened file's records could not be re-read for deletion",
            Self::ForeignChild => "Another node is parented under this one",
            Self::PerFaceShape => "A shape of this node carries per-face material links",
            Self::NextAvailableIndex => {
                "A shape of this node is linked to a set by a next-available index"
            }
            Self::NamedElsewhere => "The file refers to this node by name from another record",
            Self::TextReference => "A script or text value in the file mentions this node",
            Self::UnrecognisedConnection => {
                "An unrecognised connection record in the file mentions this node"
            }
            Self::TextScanLimit => {
                "The file's text is too large to check for mentions of this node"
            }
            Self::UnexpectedDrop => "The prune would drop a record that is not part of this node",
            Self::Alignment => "Removing this node would change the padding of its file group",
            Self::LastDiffuseUser => "This node holds the only use of a shader",
            Self::PendingInsertion => "Another node has an edit that still needs a record inserted",
            Self::LastMaterialUser => "This node holds the only use of a material",
            Self::FirstMaterialUser => "Deleting this node would renumber the remaining materials",
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

/// Whether any `|`- or `:`-separated component of the node part is one of `names`, so `|a`,
/// `:|a` and `|a|as` all count as naming `a`.
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

/// `(node, attribute)` of an `owner.attribute` endpoint; the node has no leading `:`.
fn endpoint<'a>(facts: &'a WireFacts<'_>, span: &Range<usize>) -> (&'a [u8], &'a [u8]) {
    let text = text(facts, span);
    let dot = text.iter().position(|byte| *byte == b'.');
    (
        plain(node_part(text)),
        dot.map_or(&[][..], |dot| &text[dot + 1..]),
    )
}

/// Checks that the transform `transform` and its exclusive `shapes` can be dropped from `bytes`
/// without leaving a record that names them, and that the byte ranges to drop keep every enclosing
/// group's padding. Ids are the opener's binding order, `NativeId = binding index + 1`.
pub(in crate::native_scene) fn check_removal(
    bytes: &Arc<[u8]>,
    ids: &[NativeId],
) -> Result<Vec<PruneRange>, RemovalRefusal> {
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
    if ids.is_empty() {
        return Err(RemovalRefusal::Unreadable);
    }
    let mut removed = Vec::with_capacity(ids.len());
    for id in ids {
        removed.push(record_of(*id)?);
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
    // A removed shape hangs under a removed transform; a removed transform has no parent.
    if removed.iter().any(
        |index| match (form(&facts, *index), parents[*index].map(plain)) {
            (Some(f), Some(parent)) if f == *b"DMSH" => !names.contains(&parent),
            (Some(f), None) if f == *b"XFRM" => false,
            _ => true,
        },
    ) {
        return Err(RemovalRefusal::ForeignChild);
    }
    if parents.iter().enumerate().any(|(index, parent)| {
        !removed.contains(&index) && parent.is_some_and(|parent| names_path(&names, parent))
    }) {
        return Err(RemovalRefusal::ForeignChild);
    }
    let creators: HashMap<&[u8], usize> = facts
        .records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| match &record.data {
            Data::Create(create) => Some((plain(text(&facts, &create.name)), index)),
            _ => None,
        })
        .collect();
    let mut chunks: Vec<usize> = removed
        .iter()
        .map(|index| facts.records[*index].group)
        .collect();
    // `(set node, mode)` of each dropped set-membership link.
    let mut dropped_sets: Vec<(&[u8], u8)> = Vec::new();
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
            Data::Connection(connection) => {
                let any = [&connection.source, &connection.destination].map(named);
                if any == [false, false] {
                    continue;
                }
                let (source, destination) = (
                    endpoint(&facts, &connection.source),
                    endpoint(&facts, &connection.destination),
                );
                let exact = [source, destination].map(|(node, _)| names.contains(&node));
                if exact == [true, true] {
                    chunks.push(record.chunk);
                    continue;
                }
                if exact == [true, false] {
                    let set_ok = match creators.get(destination.0) {
                        Some(index) => {
                            matches!(form(&facts, *index), Some(f) if f == *b"SHAD" || f == *b"OBST")
                        }
                        None => text(&facts, &connection.destination).starts_with(b":"),
                    };
                    if set_ok
                        && (normalized(source.1, &[b"iog"])
                            || normalized(source.1, &[b"iog", b"og"]))
                        && normalized(destination.1, &[b"dsm"])
                    {
                        // Unindexed mode 1 is an unordered set member (PROVISIONAL, #5): load
                        // appends it, so nothing renumbers. An indexed mode-1 link is refused.
                        if connection.mode != 0 && destination.1 != b"dsm" {
                            return Err(RemovalRefusal::NextAvailableIndex);
                        }
                        dropped_sets.push((destination.0, connection.mode));
                        chunks.push(record.chunk);
                        continue;
                    }
                }
                let per_face = exact[1]
                    && (normalized(destination.1, &[b"iog", b"og", b"gid"])
                        || normalized(destination.1, &[b"iog", b"og", b"gco"]));
                return Err(if per_face {
                    RemovalRefusal::PerFaceShape
                } else {
                    RemovalRefusal::NamedElsewhere
                });
            }
            _ => {}
        }
    }
    for record in &facts.records {
        let Data::Connection(connection) = &record.data else {
            continue;
        };
        if chunks.contains(&record.chunk) {
            continue;
        }
        let (node, attribute) = endpoint(&facts, &connection.destination);
        for (_, dropped) in dropped_sets.iter().filter(|(set, _)| *set == node) {
            if normalized(attribute, &[b"dagSetMembers"]) {
                return Err(RemovalRefusal::NamedElsewhere);
            }
            let indexed = attribute != b"dsm";
            if normalized(attribute, &[b"dsm"])
                && ((*dropped == 0 && connection.mode == 1) || (*dropped == 1 && indexed))
            {
                return Err(RemovalRefusal::NextAvailableIndex);
            }
        }
    }
    scan_text(&container, &chunks, &names, MAX_TEXT_WORK)?;
    prune_ranges(&container, &chunks)
}

/// A byte range to drop, and the header starts of the groups whose size words shrink with it.
pub(in crate::native_scene) struct PruneRange {
    pub(in crate::native_scene) range: Range<usize>,
    pub(in crate::native_scene) ancestors: Vec<usize>,
}

/// Bytes times names that [`scan_text`] may compare; more is refused rather than skipped.
const MAX_TEXT_WORK: usize = 16 * 1024 * 1024;

/// Whether `name` occurs in `text` with no identifier byte (`[A-Za-z0-9_]`) on either side.
fn mentions(text: &[u8], name: &[u8]) -> bool {
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    text.windows(name.len()).enumerate().any(|(at, window)| {
        window == name
            && (at == 0 || !word(text[at - 1]))
            && text.get(at + name.len()).is_none_or(|byte| !word(*byte))
    })
}

/// Refuses when a surviving `STR ` chunk, a leaf of a `SCRP` group or a leaf of the `CONN` list
/// that the opener does not decode mentions a removed name. Numeric payloads and the leaves of
/// other unbound groups are not scanned (PROVISIONAL). Work is charged as bytes times names
/// against `limit`.
fn scan_text(
    container: &Container,
    removed: &[usize],
    names: &[&[u8]],
    limit: usize,
) -> Result<(), RemovalRefusal> {
    if names.iter().any(|name| name.is_empty()) {
        return Err(RemovalRefusal::Unreadable);
    }
    let dropped: Vec<Range<usize>> = removed
        .iter()
        .filter_map(|index| container.chunk(*index))
        .map(|chunk| chunk.header_range().start..chunk.padding_range().end)
        .collect();
    let in_script = |mut parent: Option<usize>| {
        while let Some(group) = parent.and_then(|index| container.chunk(index)) {
            if group.form() == Some(*b"SCRP") {
                return true;
            }
            parent = group.parent();
        }
        false
    };
    let in_connections = |parent: Option<usize>| {
        parent
            .and_then(|index| container.chunk(index))
            .is_some_and(|group| group.form() == Some(*b"CONN"))
    };
    let mut work = 0usize;
    for (index, chunk) in container.chunks().iter().enumerate() {
        let start = chunk.header_range().start;
        let text_leaf = chunk.tag() == *b"STR " || in_script(chunk.parent());
        let skipped_link = !semantic::recognized(chunk.tag()) && in_connections(chunk.parent());
        if chunk.form().is_some()
            || !(text_leaf || skipped_link)
            || dropped.iter().any(|range| range.contains(&start))
        {
            continue;
        }
        let text = container.payload(index).ok_or(RemovalRefusal::Unreadable)?;
        work = text
            .len()
            .checked_mul(names.len())
            .and_then(|cost| work.checked_add(cost))
            .filter(|work| *work <= limit)
            .ok_or(RemovalRefusal::TextScanLimit)?;
        if names.iter().any(|name| mentions(text, name)) {
            return Err(if text_leaf {
                RemovalRefusal::TextReference
            } else {
                RemovalRefusal::UnrecognisedConnection
            });
        }
    }
    Ok(())
}

/// Every removed range must keep each enclosing group's padded size consistent: its length must be
/// a multiple of the child alignment of every ancestor frame, and ranges must not overlap. Returns
/// the ranges ascending.
fn prune_ranges(
    container: &Container,
    chunks: &[usize],
) -> Result<Vec<PruneRange>, RemovalRefusal> {
    let mut ranges = Vec::with_capacity(chunks.len());
    for index in chunks {
        let chunk = container.chunk(*index).ok_or(RemovalRefusal::Unreadable)?;
        let range = chunk.header_range().start..chunk.padding_range().end;
        let mut ancestors = Vec::new();
        let mut ancestor = chunk.parent();
        while let Some(parent) = ancestor.and_then(|index| container.chunk(index)) {
            let alignment = parent
                .form()
                .and_then(|form| container::group_alignment(parent.tag(), form))
                .ok_or(RemovalRefusal::Unreadable)?;
            if !(range.end - range.start).is_multiple_of(alignment) {
                return Err(RemovalRefusal::Alignment);
            }
            ancestors.push(parent.header_range().start);
            ancestor = parent.parent();
        }
        ranges.push(PruneRange { range, ancestors });
    }
    ranges.sort_unstable_by_key(|prune| prune.range.start);
    if ranges
        .windows(2)
        .any(|pair| pair[0].range.end > pair[1].range.start)
    {
        return Err(RemovalRefusal::Unreadable);
    }
    Ok(ranges)
}

/// Independent of how `check_removal` chose its ranges: each must be exactly the group of a removed
/// binding or a connection with a removed endpoint, and the pruned bytes must hold the original's
/// records minus the ones inside the ranges.
pub(super) fn check_dropped(
    original: &Arc<[u8]>,
    ids: &[NativeId],
    ranges: &[PruneRange],
    pruned: &Arc<[u8]>,
) -> Result<(), RemovalRefusal> {
    let container = container::inspect(original.clone()).map_err(|_| RemovalRefusal::Unreadable)?;
    let facts = semantic::decode_records(&container).map_err(|_| RemovalRefusal::Unreadable)?;
    let bound: Vec<usize> = (0..facts.records.len())
        .filter(|index| is_bound(&container, &facts.records[*index]))
        .collect();
    let mut groups = Vec::with_capacity(ids.len());
    let mut names: Vec<&[u8]> = Vec::with_capacity(ids.len());
    for id in ids {
        let index = usize::try_from(id.0)
            .ok()
            .and_then(|id| id.checked_sub(1))
            .and_then(|id| bound.get(id))
            .ok_or(RemovalRefusal::Unreadable)?;
        let Data::Create(create) = &facts.records[*index].data else {
            return Err(RemovalRefusal::Unreadable);
        };
        groups.push(facts.records[*index].group);
        names.push(plain(text(&facts, &create.name)));
    }
    let removed_link = |chunk: usize| {
        facts.records.iter().any(|record| {
            record.chunk == chunk
                && matches!(&record.data, Data::Connection(link)
                    if [&link.source, &link.destination]
                        .into_iter()
                        .any(|span| names.contains(&endpoint(&facts, span).0)))
        })
    };
    for prune in ranges {
        let chunk = container
            .chunks()
            .iter()
            .position(|chunk| {
                chunk.header_range().start == prune.range.start
                    && chunk.padding_range().end == prune.range.end
            })
            .ok_or(RemovalRefusal::UnexpectedDrop)?;
        if !groups.contains(&chunk) && !removed_link(chunk) {
            return Err(RemovalRefusal::UnexpectedDrop);
        }
    }
    let inside = |chunk: usize| {
        let start = container.chunks()[chunk].header_range().start;
        ranges.iter().any(|prune| prune.range.contains(&start))
    };
    let dropped = facts.records.iter().filter(|r| inside(r.chunk)).count();
    let after = container::inspect(pruned.clone()).map_err(|_| RemovalRefusal::Unreadable)?;
    let kept = semantic::decode_records(&after)
        .map_err(|_| RemovalRefusal::Unreadable)?
        .records
        .len();
    if kept != facts.records.len() - dropped {
        return Err(RemovalRefusal::UnexpectedDrop);
    }
    Ok(())
}

/// The pruned file must not name a removed node anywhere a record can: a surviving creation,
/// selection, relation, connection, text leaf or unrecognised connection leaf that does is a bug
/// in the prune, not a user error.
pub(super) fn assert_pruned(bytes: &Arc<[u8]>, names: &[&[u8]]) -> Result<(), RemovalRefusal> {
    let container = container::inspect(bytes.clone()).map_err(|_| RemovalRefusal::Unreadable)?;
    let facts = semantic::decode_records(&container).map_err(|_| RemovalRefusal::Unreadable)?;
    let named = |span: &Range<usize>| names_path(names, text(&facts, span));
    let found = facts.records.iter().any(|record| match &record.data {
        Data::Create(create) => {
            names.contains(&plain(text(&facts, &create.name)))
                || create.parent.as_ref().is_some_and(named)
        }
        Data::Selection(target) => named(target),
        Data::Relation(relation) => [&relation.head, &relation.tails[0], &relation.tails[1]]
            .into_iter()
            .any(named),
        Data::Connection(connection) => named(&connection.source) || named(&connection.destination),
        _ => false,
    });
    if found {
        return Err(RemovalRefusal::NamedElsewhere);
    }
    scan_text(&container, &[], names, MAX_TEXT_WORK)
}

/// A removed shape must not be the only user of a material, and must not be the first user of one
/// that other shapes keep: reopening numbers materials by first use, so either would renumber them.
pub(in crate::native_scene) fn check_materials(
    appearance: &SourceAppearance,
    removed: &[NativeId],
) -> Result<(), RemovalRefusal> {
    for shape in removed {
        let Some(Ok(binding)) = appearance
            .shapes
            .iter()
            .find(|entry| entry.shape == *shape)
            .map(|entry| &entry.binding)
        else {
            continue;
        };
        let users: Vec<NativeId> = appearance
            .shapes
            .iter()
            .filter(|entry| {
                entry
                    .binding
                    .as_ref()
                    .is_ok_and(|b| b.material == binding.material)
            })
            .map(|entry| entry.shape)
            .collect();
        if users.iter().all(|user| removed.contains(user)) {
            return Err(RemovalRefusal::LastMaterialUser);
        }
        if removed.contains(&users[0]) {
            return Err(RemovalRefusal::FirstMaterialUser);
        }
    }
    // A diffuse-bound shader left without a user would not be listed after a reopen.
    if appearance.diffuse.iter().any(|binding| {
        !binding.shapes.is_empty() && binding.shapes.iter().all(|user| removed.contains(user))
    }) {
        return Err(RemovalRefusal::LastDiffuseUser);
    }
    Ok(())
}

#[cfg(test)]
mod scene_tests;
#[cfg(test)]
pub(super) mod tests;
