//! Byte-compared text snapshots under `tests/snapshots`.
//!
//! `UPDATE_SNAPSHOTS=1` rewrites a snapshot; every other run fails on any difference, including a
//! missing file. Snapshots are public, so callers normalize machine-specific values first.

use std::path::PathBuf;

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join(name)
}

pub(crate) fn assert_snapshot(name: &str, actual: &str) {
    assert_text_file(&path(name), name, actual, "UPDATE_SNAPSHOTS");
}

/// Byte-compares `actual` with the committed text file at `path`, ignoring CRLF from a checkout
/// that applied autocrlf. `update_var=1` rewrites the file instead.
pub(crate) fn assert_text_file(path: &std::path::Path, name: &str, actual: &str, update_var: &str) {
    if std::env::var_os(update_var).is_some_and(|value| value == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, actual).unwrap();
        return;
    }
    let expected = std::fs::read(path).unwrap_or_else(|error| {
        panic!("{name} is unreadable ({error}); regenerate with {update_var}=1 and review the diff")
    });
    // A checkout may apply autocrlf despite .gitattributes; line endings are not content.
    let expected = String::from_utf8_lossy(&expected).replace("\r\n", "\n");
    if let Some(difference) = first_difference(&expected, actual) {
        eprintln!("DBG-BEGIN {name}\n{actual}\n DBG-END {name}");
        panic!(
            "{name} differs, {difference}\nregenerate with {update_var}=1 only if the change is intended"
        );
    }
}

/// Describes the first difference between two LF-normalized texts, or `None` when equal. Never
/// reports a difference without saying what differs, including final-newline and trailing bytes.
fn first_difference(expected: &str, actual: &str) -> Option<String> {
    if expected == actual {
        return None;
    }
    let mut left = expected.split('\n');
    let mut right = actual.split('\n');
    let mut line = 1;
    loop {
        match (left.next(), right.next()) {
            (Some(l), Some(r)) if l == r => line += 1,
            (Some(l), Some(r)) => {
                return Some(format!(
                    "first at line {line}:\n  expected: {l:?}\n  actual:   {r:?}"
                ));
            }
            (Some(_), None) | (None, Some(_)) => {
                return Some(format!(
                    "line counts differ at line {line}: expected {} bytes, actual {} bytes (final newline or trailing content)",
                    expected.len(),
                    actual.len()
                ));
            }
            (None, None) => return Some("texts differ in an unclassified way".into()),
        }
    }
}

#[test]
fn reporter_normalizes_nothing_it_cannot_explain() {
    assert!(first_difference("a\nb\n", "a\nb\n").is_none());
    let message = first_difference("a\nb\n", "a\nb").unwrap();
    assert!(message.contains("final newline"), "{message}");
    let message = first_difference("a\nb", "a\nb\nc\n").unwrap();
    assert!(
        message.contains("final newline") || message.contains("line"),
        "{message}"
    );
    let message = first_difference("a\nb\n", "a\nx\n").unwrap();
    assert!(
        message.contains("line 2") && message.contains("\"b\""),
        "{message}"
    );
    assert!(!message.contains("<end>"));
}

#[test]
fn crlf_committed_text_compares_equal_after_normalization() {
    let committed = "a\r\nb\r\n";
    assert!(first_difference(&committed.replace("\r\n", "\n"), "a\nb\n").is_none());
    // A CR that is not part of CRLF is content and still differs.
    assert!(first_difference("a\rb\n", "a\nb\n").is_some());
}

fn is_uuid(bytes: &[u8]) -> bool {
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// Replaces UUIDs (scene tokens, pending-action ids) and the given absolute roots, then refuses
/// text that still looks machine-specific so a snapshot cannot leak a home or temp path.
pub(crate) fn normalize_text(text: &str, roots: &[&std::path::Path]) -> String {
    let mut text = text.to_owned();
    for root in roots {
        let root = root.to_str().unwrap();
        text = text.replace(root, "<dir>");
        if let Ok(canonical) = std::fs::canonicalize(root) {
            text = text.replace(canonical.to_str().unwrap(), "<dir>");
        }
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if index + 36 <= bytes.len()
            && text.is_char_boundary(index + 36)
            && is_uuid(&bytes[index..index + 36])
        {
            out.push_str("<uuid>");
            index += 36;
        } else {
            let ch = text[index..].chars().next().unwrap();
            out.push(ch);
            index += ch.len_utf8();
        }
    }
    for forbidden in ["/Users/", "/home/", "/private/", "/tmp", "/var/", "C:\\"] {
        assert!(
            !out.contains(forbidden),
            "snapshot text contains a machine-specific path ({forbidden})"
        );
    }
    out
}

#[cfg(feature = "mcp-test-client")]
pub(crate) fn normalize_json(
    value: &serde_json::Value,
    roots: &[&std::path::Path],
) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(normalize_text(text, roots)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| normalize_json(v, roots)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| (key.clone(), normalize_json(v, roots)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn crlf_checkout_of_a_committed_file_passes_and_real_drift_fails() {
    let dir = std::env::temp_dir().join(format!("azteccraft-textfile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("doc.md");
    std::fs::write(&file, "a\r\n\r\nb\r\n").unwrap();
    assert_text_file(&file, "doc", "a\n\nb\n", "AZTECCRAFT_UNSET_UPDATE_VAR");
    let drift = std::panic::catch_unwind(|| {
        assert_text_file(&file, "doc", "a\n\nc\n", "AZTECCRAFT_UNSET_UPDATE_VAR");
    });
    assert!(drift.is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}
