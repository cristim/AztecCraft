//! Entry-point gate for the operation pipelines.
//!
//! Claim: every `Operation` mutates the editor only through `apply_operation_guarded` (reached
//! natively via `apply_operation` and remotely via `remote_operation_guarded`), so the guard
//! order recorded in the guard matrix cannot be bypassed.
//!
//! What is checked, over the production text of every binary-crate file under `src/` (library
//! modules from `lib.rs` and test files are skipped; text before a file's first
//! `#[cfg(test)] mod`, `\r\n` normalized, `//` comments removed), keyed by function name:
//! 1. `apply_operation_guarded(` is called only from `apply_operation` and
//!    `remote_operation_guarded`.
//! 2. `remote_operation_guarded(` is called only from `remote_operation` (a test wrapper) and the
//!    MCP bridge, and nothing under `src/mcp*` reaches the editor any other way.
//! 3. Every call site of the history mutators (`.execute(`, `.undo()`, `.redo()`,
//!    `.begin_move(`, `.update_move(`, `.finish_move(`, `.cancel_active_move(`, `History::new(`)
//!    and every `self.history =` assignment sits in an allowlisted function.
//!
//! Sources are found by walking `src/`, not by a fixed list, so `app.rs -> app/mod.rs` and files
//! added by later splits are covered without edits here.
//!
//! Why it is sound enough: the editor state is private to `app`, so a mutation needs one of these
//! textual call forms in a function that is named here. A new caller changes the discovered set
//! and fails the test until a reviewer adds it. Limits: it is textual. Macros, aliasing a
//! mutator through a closure or function pointer, or a `std::mem::swap` of `history` evade it,
//! and function identity is the nearest preceding `fn` name (nested closures count as their
//! parent). Treat a failure as a prompt to re-read the guard ledger, not as proof of a bug.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Library modules (declared in `lib.rs`) cannot reach `Editor`, so they are not scanned.
fn library_modules() -> BTreeSet<String> {
    let text = std::fs::read_to_string(src_dir().join("lib.rs")).unwrap();
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub ").unwrap_or(line.trim());
            let rest = rest.strip_prefix("(crate) ").unwrap_or(rest);
            let name = rest.strip_prefix("mod ")?.strip_suffix(';')?;
            Some(name.trim().to_owned())
        })
        .collect()
}

fn is_test_file(relative: &Path) -> bool {
    let name = relative.file_name().unwrap().to_str().unwrap();
    name == "tests.rs"
        || name == "test_fixtures.rs"
        || name == "snapshots.rs"
        || name.ends_with("_tests.rs")
        || relative
            .components()
            .any(|part| part.as_os_str() == "tests" || part.as_os_str() == "mcp_tests")
}

fn collect(directory: &Path, root: &Path, library: &BTreeSet<String>, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let relative = path.strip_prefix(root).unwrap();
        let top = relative
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_str()
            .unwrap();
        let top = top.strip_suffix(".rs").unwrap_or(top);
        if library.contains(top) {
            continue;
        }
        if path.is_dir() {
            collect(&path, root, library, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") && !is_test_file(relative) {
            out.push(relative.to_path_buf());
        }
    }
}

/// Every binary-crate source file under `src/`, so a file added later cannot hide a bypass.
fn files() -> Vec<PathBuf> {
    let root = src_dir();
    let mut out = Vec::new();
    collect(&root, &root, &library_modules(), &mut out);
    assert!(
        out.iter().any(|path| path.ends_with("mcp.rs")) && out.len() > 5,
        "source discovery found {out:?}"
    );
    out
}

/// Production text of one source: `\r\n` normalized (Windows checkouts), everything from the
/// first `#[cfg(test)] mod` dropped, `//` comments removed.
fn production_text(source: &str) -> String {
    let source = source.replace("\r\n", "\n");
    let end = source.find("\n#[cfg(test)]\nmod ").unwrap_or(source.len());
    source[..end]
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn production(file: &Path) -> String {
    production_text(&std::fs::read_to_string(src_dir().join(file)).unwrap())
}

#[test]
fn production_text_handles_crlf_and_drops_test_modules() {
    let source = "fn keep() {\r\n    a.undo(); // note\r\n}\r\n#[cfg(test)]\r\nmod tests {\r\n    fn drop() { b.undo(); }\r\n}\r\n";
    let text = production_text(source);
    assert!(text.contains("fn keep()") && text.contains("a.undo();"));
    assert!(!text.contains("note") && !text.contains("drop") && !text.contains('\r'));
}

fn defined_fn(line: &str) -> Option<String> {
    let mut rest = line.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if rest.starts_with('(') {
            rest = rest[rest.find(')')? + 1..].trim_start();
        }
    }
    for qualifier in ["const ", "async ", "unsafe "] {
        rest = rest.strip_prefix(qualifier).unwrap_or(rest);
    }
    let name: String = rest
        .strip_prefix("fn ")?
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Names of the functions containing `needle`, excluding the `fn needle` definition itself.
/// Keyed by function name, not file, so a pure file move does not change the result.
fn sites(needle: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for file in files() {
        let mut current = "<module>".to_owned();
        for line in production(&file).lines() {
            if let Some(name) = defined_fn(line) {
                current = name;
            }
            if line.contains(needle) && !line.contains(&format!("fn {needle}")) {
                found.insert(current.clone());
            }
        }
    }
    found
}

fn expect(label: &str, needle: &str, allowed: &[&str]) {
    let found = sites(needle);
    let allowed: BTreeSet<String> = allowed.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        found, allowed,
        "{label}: call sites of `{needle}` changed; review the guard ledger before updating"
    );
}

#[test]
fn only_the_two_pipelines_call_apply_operation_guarded() {
    expect(
        "guarded dispatch",
        "apply_operation_guarded(",
        &["apply_operation", "remote_operation_guarded"],
    );
}

#[test]
fn remote_pipeline_is_entered_only_by_the_mcp_bridge() {
    expect(
        "remote dispatch",
        "remote_operation_guarded(",
        &["remote_operation", "dispatch"],
    );
    for file in files()
        .into_iter()
        .filter(|file| file.to_str().unwrap().starts_with("mcp"))
    {
        let text = production(&file);
        for forbidden in [
            "apply_operation",
            "native_operation",
            ".history",
            "History::",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} reaches the editor through `{forbidden}` instead of remote_operation_guarded",
                file.display()
            );
        }
    }
}

#[test]
fn native_operation_callers_are_the_known_ui_handlers() {
    expect(
        "native wrapper",
        "apply_operation(",
        &[
            "native_operation",
            "viewport",
            "file_modal",
            "finish_native_sample",
            "handle_move_input",
            "start_native_drag",
            "begin_native_manipulation",
            "run_numeric_manipulation",
        ],
    );
    expect(
        "native UI wrapper",
        "native_operation(",
        &[
            "create",
            "create_from_dialog",
            "file_actions",
            "frame_all",
            "frame_selected",
            "history_step",
            "manipulation_options",
            "outliner",
            "render",
            "select",
            "shortcuts",
            "viewport",
            "open_path",
            "request_close",
            "request_new",
            "save_as_dialog",
        ],
    );
}

#[test]
fn tool_handle_setter_is_reached_only_through_dispatch() {
    expect(
        "tool handle setter",
        ".set_tool_handle(",
        &["apply_operation_guarded"],
    );
    expect(
        "tool handle combos",
        "native_operation(Operation::SetToolHandle(",
        &["manipulation_options"],
    );
}

#[test]
fn history_mutators_live_only_in_the_dispatch_and_move_handlers() {
    // commit_pending commits a human draft; it is the one documented non-Operation mutation.
    expect(
        "history execute",
        ".execute(",
        &["apply_operation_guarded", "commit_pending"],
    );
    expect("history undo", ".undo()", &["apply_operation_guarded"]);
    expect("history redo", ".redo()", &["apply_operation_guarded"]);
    expect("move begin", ".begin_move(", &["begin_editor_move"]);
    expect("rotate begin", ".begin_rotate(", &["begin_editor_rotate"]);
    expect("scale begin", ".begin_scale(", &["begin_editor_scale"]);
    expect("move update", ".update_move(", &["update_editor_move"]);
    expect(
        "rotate update",
        ".update_move_declared(",
        &["update_editor_manipulation"],
    );
    expect("move finish", ".finish_move(", &["finish_editor_move"]);
    expect("move cancel", ".cancel_active_move(", &["cancel_move"]);
    expect("history replace", "self.history =", &["replace_scene"]);
    expect("history new", "History::new(", &["replace_scene"]);
}
