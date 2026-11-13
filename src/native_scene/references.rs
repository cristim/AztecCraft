//! Inert, read-only reference declarations parsed from Maya ASCII `file -r ...` records.
//!
//! Nothing here loads, resolves or stats a referenced path: paths are untrusted scene data. The
//! fields come from the `file` command page of the pinned Maya documentation (#440); anything it
//! does not define is refused by the parser, not guessed. PROVISIONAL until the pinned reference
//! install exists (#5).

use serde::Serialize;

/// At most this many declarations per scene (refused with a resource limit beyond it).
pub const MAX_REFERENCES: usize = 32;
/// Longest namespace, node, prefix or type string accepted from a declaration.
pub const MAX_REFERENCE_TEXT_BYTES: usize = 128;
/// Longest authored path accepted (a longer one is a resource limit). A relative path is echoed;
/// an absolute or escaping one never is.
pub const MAX_REFERENCE_PATH_BYTES: usize = 256;

/// How the authored path points, judged on the text only (the filesystem is never consulted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferencePathKind {
    /// No root, drive, home or `..` segment.
    Relative,
    /// Starts with `/` (also `//host`), `~`, `$` (an environment variable that may expand to a home
    /// directory) or a drive letter and colon; it can carry a user name, so it is
    /// not reported.
    Absolute,
    /// Any `/`-separated `..` segment; not reported.
    Escaping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReferenceDeclaration {
    pub path_kind: ReferencePathKind,
    /// The authored path, only when `path_kind` is relative.
    pub path: Option<String>,
    /// `-ns`.
    pub namespace: Option<String>,
    /// `-rfn`.
    pub reference_node: Option<String>,
    /// `-rpr` (the page says `-ns` replaced it).
    pub renaming_prefix: Option<String>,
    /// `-typ`.
    pub file_type: Option<String>,
    /// `-dr`: whether loading is deferred, as declared.
    pub deferred: Option<bool>,
    /// `-rdi`: stored reference loading preference, as declared.
    pub depth_info: Option<u32>,
}

/// Classifies an authored path by its text alone. Anything that could name a location outside the
/// document directory, or expand to one (`:` scheme or drive, `%VAR%`, `$VAR`, `//host`, `~`), is
/// absolute and never echoed.
pub(crate) fn path_kind(path: &str) -> ReferencePathKind {
    let rooted =
        path.starts_with(['/', '~', '$']) || path.contains([':', '%']) || path.contains("//");
    if rooted {
        ReferencePathKind::Absolute
    } else if path.split('/').any(|part| part == "..") {
        ReferencePathKind::Escaping
    } else {
        ReferencePathKind::Relative
    }
}

/// Whether a declared string may be stored: printable ASCII punctuation, letters, digits and
/// spaces only (no control, format or bidirectional characters), no backslash escape, no edge
/// space.
pub(crate) fn printable(text: &str) -> bool {
    !text.starts_with(' ')
        && !text.ends_with(' ')
        && !text.contains('\\')
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_ascii_punctuation() || c == ' ')
}

/// Whether a declared name or option string may be stored.
pub(crate) fn text_ok(text: &str) -> bool {
    text.len() <= MAX_REFERENCE_TEXT_BYTES && printable(text)
}
