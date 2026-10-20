//! The vendored public Maya files the writer profile is pinned to, and a small
//! independent walker over their bytes.
//!
//! The repo's container reader rejects group forms the Maya 2025 file contains, so the
//! pin tests read headers directly: tag, aux (big endian), size (big endian u64), and for
//! groups a four-byte form; children align to 4 in `Maya` and 8 in every other group.

use std::ops::Range;

/// Maya 2025, the writer's profile source. `tests/fixtures/gt-tools/UPSTREAM.md`.
pub(super) const MAYA_2025: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/gt-tools/dummy_mesh.mb"
));
/// Maya 2026, read tests only; the writer takes nothing from it.
pub(super) const MAYA_2026: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/mayascenekit/file_owner_delete.mb"
));

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub tag: [u8; 4],
    /// `Some` for a `FOR8` group.
    pub form: Option<[u8; 4]>,
    pub aux: u32,
    pub start: usize,
    pub payload: Range<usize>,
    /// Header, payload and padding to the parent's alignment.
    pub end: usize,
    pub parent_form: Option<[u8; 4]>,
}

impl Entry {
    /// Payload of a leaf, or children of a group after its form.
    pub(super) fn bytes<'a>(&self, file: &'a [u8]) -> &'a [u8] {
        &file[self.payload.clone()]
    }
    pub(super) fn whole<'a>(&self, file: &'a [u8]) -> &'a [u8] {
        &file[self.start..self.end]
    }
}

/// `N` bytes at `at`, with a panic naming the offset when the file ends first.
fn array<const N: usize>(file: &[u8], at: usize) -> [u8; N] {
    at.checked_add(N)
        .and_then(|end| file.get(at..end))
        .and_then(|bytes| bytes.try_into().ok())
        .unwrap_or_else(|| panic!("file ends before {N} bytes at {at:#x}"))
}

pub(super) fn walk(file: &[u8]) -> Vec<Entry> {
    let mut out = Vec::new();
    descend(file, 0, file.len(), 1, None, &mut out);
    out
}

fn descend(
    file: &[u8],
    mut at: usize,
    end: usize,
    align: usize,
    parent_form: Option<[u8; 4]>,
    out: &mut Vec<Entry>,
) {
    while at
        .checked_add(16)
        .is_some_and(|header_end| header_end <= end)
    {
        let tag: [u8; 4] = array(file, at);
        let aux = u32::from_be_bytes(array(file, at + 4));
        let size = u64::from_be_bytes(array(file, at + 8));
        let Some((size, payload_end)) = usize::try_from(size)
            .ok()
            .and_then(|size| Some((size, (at + 16).checked_add(size)?)))
        else {
            panic!("chunk at {at:#x} has an unrepresentable size {size}");
        };
        let payload = at + 16..payload_end;
        assert!(payload.end <= end, "chunk at {at:#x} overruns its parent");
        let padded = (16 + size).next_multiple_of(align);
        let form = (tag == *b"FOR8").then(|| array(file, payload.start));
        out.push(Entry {
            tag,
            form,
            aux,
            start: at,
            payload: payload.clone(),
            end: at + padded,
            parent_form,
        });
        if let Some(form) = form {
            let child_align = if form == *b"Maya" { 4 } else { 8 };
            descend(
                file,
                payload.start + 4,
                payload.end,
                child_align,
                Some(form),
                out,
            );
        }
        at += padded;
    }
}

/// Every leaf `tag` whose parent group has form `parent`.
pub(super) fn leaves(entries: &[Entry], tag: &[u8; 4], parent: &[u8; 4]) -> Vec<Entry> {
    entries
        .iter()
        .filter(|e| e.tag == *tag && e.form.is_none() && e.parent_form == Some(*parent))
        .cloned()
        .collect()
}

/// Every group `FOR8 <form>`.
pub(super) fn groups(entries: &[Entry], form: &[u8; 4]) -> Vec<Entry> {
    entries
        .iter()
        .filter(|e| e.form == Some(*form))
        .cloned()
        .collect()
}
