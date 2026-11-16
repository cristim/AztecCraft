//! Vocabulary of the bounded typed protocol between the editor and the `script-host` helper (#57):
//! limits, [`ProtocolError`] and the closed [`ScriptCall`] set with its argument validation.
//!
//! Pure: no I/O, no process, no clock. The helper runs a guest under wasmtime and is treated as
//! hostile (a compiler bug can turn the guest into the helper), so every decode path returns a
//! typed [`ProtocolError`] and never panics; errors carry sizes and kinds, never frame text.
//! Framing and the run state machine live in `frame`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Longest object name a call may carry; mirrors `editor_operations::MAX_NAME_BYTES`, which is
/// compiled only with `mcp` until the `scripting` feature ungates it (T2a-1) and this can import it.
pub const MAX_NAME_BYTES: usize = 4096;
/// Helper to editor frame bound (the MCP ingress bound, `src/mcp/transport.rs`).
pub const MAX_HELPER_FRAME_BYTES: usize = 64 * 1024;
/// Editor to helper frame bound (the MCP egress bound).
pub const MAX_EDITOR_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_CALLS_PER_RUN: u32 = 10_000;
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
/// Every helper frame counts, `output` frames included.
pub const MAX_FRAMES_PER_RUN: u32 = 20_000;
/// Deepest `[`/`{` nesting a helper frame may use; the real frames need 3.
pub const MAX_JSON_DEPTH: usize = 8;
/// PROVISIONAL bound on a script-supplied transform number; the editor operation validates further.
pub const MAX_SCRIPT_NUMBER_ABS: f64 = 1.0e9;
/// Longest error text a `done` frame may carry.
pub const MAX_DONE_ERROR_BYTES: usize = 4 * 1024;

/// Why a frame or a run was rejected. Variants never hold frame text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// The length prefix exceeds the direction's bound (checked before any payload is buffered).
    FrameTooLarge {
        len: usize,
        max: usize,
    },
    EmptyFrame,
    /// The stream ended inside a prefix or payload.
    Truncated,
    NotUtf8,
    /// Nesting deeper than [`MAX_JSON_DEPTH`].
    TooDeep,
    NotJson,
    NotAnObject,
    MissingType,
    UnknownFrameType,
    UnknownCall,
    /// Wrong, missing, unknown or out-of-range fields for a known frame or call.
    InvalidFields,
    InvalidSeq,
    /// `call` before the run started, or any frame after `done`.
    OutsideRun,
    /// A second `call` while one is unanswered.
    SecondOutstandingCall,
    /// A reply for a seq that is not the outstanding call.
    UnexpectedReply,
    /// `done` while a call is unanswered.
    DoneWithOutstandingCall,
    CallLimit,
    OutputLimit,
    FrameLimit,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooLarge { len, max } => write!(f, "frame of {len} bytes exceeds {max}"),
            Self::EmptyFrame => f.write_str("empty frame"),
            Self::Truncated => f.write_str("truncated frame"),
            Self::NotUtf8 => f.write_str("frame is not UTF-8"),
            Self::TooDeep => f.write_str("frame nesting too deep"),
            Self::NotJson => f.write_str("frame is not JSON"),
            Self::NotAnObject => f.write_str("frame is not a JSON object"),
            Self::MissingType => f.write_str("frame has no type"),
            Self::UnknownFrameType => f.write_str("unknown frame type"),
            Self::UnknownCall => f.write_str("unknown call name"),
            Self::InvalidFields => f.write_str("invalid fields"),
            Self::InvalidSeq => f.write_str("invalid call sequence number"),
            Self::OutsideRun => f.write_str("frame outside a run"),
            Self::SecondOutstandingCall => f.write_str("second outstanding call"),
            Self::UnexpectedReply => f.write_str("reply does not match the outstanding call"),
            Self::DoneWithOutstandingCall => f.write_str("done with an outstanding call"),
            Self::CallLimit => f.write_str("call limit exceeded"),
            Self::OutputLimit => f.write_str("output limit exceeded"),
            Self::FrameLimit => f.write_str("frame count limit exceeded"),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// The closed set of calls a script may make; each maps to one editor operation or read in the
/// broker. Names are validated against [`MAX_NAME_BYTES`]; whether they resolve is the broker's job.
#[derive(Clone, PartialEq)]
pub enum ScriptCall {
    /// `ls`: object names, or only the selection.
    Ls {
        selection_only: bool,
    },
    /// `select` replacing the selection with one object.
    Select {
        name: String,
    },
    /// `select -clear`.
    SelectClear,
    Rename {
        from: String,
        to: String,
    },
    /// `xform` on one node; at least one channel is present, rotation in degrees (PROVISIONAL).
    Xform {
        name: String,
        translate: Option<[f64; 3]>,
        rotate: Option<[f64; 3]>,
        scale: Option<[f64; 3]>,
    },
    /// `undoInfo -q`; opening or closing chunks is not callable.
    UndoInfoQuery,
}

impl fmt::Debug for ScriptCall {
    /// Names and numbers are user scene data, so only the call kind is printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ls { .. } => "ScriptCall::Ls",
            Self::Select { .. } => "ScriptCall::Select",
            Self::SelectClear => "ScriptCall::SelectClear",
            Self::Rename { .. } => "ScriptCall::Rename",
            Self::Xform { .. } => "ScriptCall::Xform",
            Self::UndoInfoQuery => "ScriptCall::UndoInfoQuery",
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LsArgs {
    #[serde(default)]
    selection_only: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectArgs {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameArgs {
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct XformArgs {
    name: String,
    translate: Option<[f64; 3]>,
    rotate: Option<[f64; 3]>,
    scale: Option<[f64; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

fn args<T: DeserializeOwned>(value: Value) -> Result<T, ProtocolError> {
    // serde would also build a struct from a JSON array.
    if !value.is_object() {
        return Err(ProtocolError::InvalidFields);
    }
    serde_json::from_value(value).map_err(|_| ProtocolError::InvalidFields)
}

fn name(text: String) -> Result<String, ProtocolError> {
    let hidden = |c: char| {
        c.is_control()
            || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' | '\u{061C}')
    };
    if text.is_empty() || text.len() > MAX_NAME_BYTES || text.chars().any(hidden) {
        return Err(ProtocolError::InvalidFields);
    }
    Ok(text)
}

fn vector(v: Option<[f64; 3]>) -> Result<Option<[f64; 3]>, ProtocolError> {
    match v {
        Some(v)
            if v.iter()
                .any(|n| !n.is_finite() || n.abs() > MAX_SCRIPT_NUMBER_ABS) =>
        {
            Err(ProtocolError::InvalidFields)
        }
        other => Ok(other),
    }
}

impl ScriptCall {
    pub(super) fn from_wire(call: &str, value: Value) -> Result<Self, ProtocolError> {
        match call {
            "ls" => Ok(Self::Ls {
                selection_only: args::<LsArgs>(value)?.selection_only,
            }),
            "select" => Ok(Self::Select {
                name: name(args::<SelectArgs>(value)?.name)?,
            }),
            "select_clear" => args::<NoArgs>(value).map(|_| Self::SelectClear),
            "rename" => {
                let a = args::<RenameArgs>(value)?;
                Ok(Self::Rename {
                    from: name(a.from)?,
                    to: name(a.to)?,
                })
            }
            "xform" => {
                let a = args::<XformArgs>(value)?;
                let (translate, rotate, scale) =
                    (vector(a.translate)?, vector(a.rotate)?, vector(a.scale)?);
                if translate.is_none() && rotate.is_none() && scale.is_none() {
                    return Err(ProtocolError::InvalidFields);
                }
                Ok(Self::Xform {
                    name: name(a.name)?,
                    translate,
                    rotate,
                    scale,
                })
            }
            "undo_info_query" => args::<NoArgs>(value).map(|_| Self::UndoInfoQuery),
            _ => Err(ProtocolError::UnknownCall),
        }
    }
}

#[cfg(test)]
mod call_tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, args: Value) -> Result<ScriptCall, ProtocolError> {
        ScriptCall::from_wire(name, args)
    }

    #[test]
    fn accepted_calls_map_to_exact_variants() {
        assert_eq!(
            call("ls", json!({})),
            Ok(ScriptCall::Ls {
                selection_only: false
            })
        );
        assert_eq!(
            call("ls", json!({"selection_only": true})),
            Ok(ScriptCall::Ls {
                selection_only: true
            })
        );
        assert_eq!(
            call("select", json!({"name": "pCube1"})),
            Ok(ScriptCall::Select {
                name: "pCube1".into()
            })
        );
        assert_eq!(call("select_clear", json!({})), Ok(ScriptCall::SelectClear));
        assert_eq!(
            call("rename", json!({"from": "a", "to": "b"})),
            Ok(ScriptCall::Rename {
                from: "a".into(),
                to: "b".into()
            })
        );
        assert_eq!(
            call("undo_info_query", json!({})),
            Ok(ScriptCall::UndoInfoQuery)
        );
        assert_eq!(
            call("xform", json!({"name": "n", "translate": [1, 2.5, -3]})),
            Ok(ScriptCall::Xform {
                name: "n".into(),
                translate: Some([1.0, 2.5, -3.0]),
                rotate: None,
                scale: None
            })
        );
    }

    #[test]
    fn rejected_calls_give_typed_errors() {
        let long = "x".repeat(MAX_NAME_BYTES + 1);
        let at_limit = "x".repeat(MAX_NAME_BYTES);
        assert!(call("select", json!({"name": at_limit})).is_ok());
        let cases: Vec<(&str, Value, ProtocolError)> = vec![
            ("eval", json!({}), ProtocolError::UnknownCall),
            ("open_undo_chunk", json!({}), ProtocolError::UnknownCall),
            ("LS", json!({}), ProtocolError::UnknownCall),
            ("", json!({}), ProtocolError::UnknownCall),
            ("ls", json!({"extra": 1}), ProtocolError::InvalidFields),
            (
                "ls",
                json!({"selection_only": "yes"}),
                ProtocolError::InvalidFields,
            ),
            ("ls", json!([]), ProtocolError::InvalidFields),
            ("ls", Value::Null, ProtocolError::InvalidFields),
            ("select", json!({}), ProtocolError::InvalidFields),
            ("select", json!({"name": ""}), ProtocolError::InvalidFields),
            (
                "select",
                json!({"name": "a\u{0}b"}),
                ProtocolError::InvalidFields,
            ),
            (
                "select",
                json!({"name": long}),
                ProtocolError::InvalidFields,
            ),
            ("select", json!({"name": 5}), ProtocolError::InvalidFields),
            (
                "select_clear",
                json!({"name": "a"}),
                ProtocolError::InvalidFields,
            ),
            ("rename", json!({"from": "a"}), ProtocolError::InvalidFields),
            ("xform", json!({"name": "n"}), ProtocolError::InvalidFields),
            (
                "xform",
                json!({"name": "n", "translate": [1, 2]}),
                ProtocolError::InvalidFields,
            ),
            (
                "xform",
                json!({"name": "n", "translate": [1, 2, 3, 4]}),
                ProtocolError::InvalidFields,
            ),
            (
                "xform",
                json!({"name": "n", "rotate": [0, 0, 2.0e9]}),
                ProtocolError::InvalidFields,
            ),
            (
                "xform",
                json!({"name": "n", "scale": ["a", 1, 1]}),
                ProtocolError::InvalidFields,
            ),
            (
                "undo_info_query",
                json!({"chunk": "open"}),
                ProtocolError::InvalidFields,
            ),
        ];
        for (name, args, want) in cases {
            assert_eq!(call(name, args.clone()), Err(want), "{name} {args}");
        }
    }

    #[test]
    fn names_reject_control_and_bidi_characters() {
        for c in "\n\t\u{7f}\u{85}\u{202a}\u{202e}\u{2066}\u{2069}\u{200e}\u{200f}\u{61c}".chars() {
            let got = call("select", json!({"name": format!("a{c}b")}));
            assert_eq!(got, Err(ProtocolError::InvalidFields), "{c:?}");
        }
        assert!(call("select", json!({"name": "caf\u{e9}_\u{4e2d}"})).is_ok());
    }

    #[test]
    fn debug_prints_the_kind_only() {
        let c = ScriptCall::Rename {
            from: "SECRET_FROM".into(),
            to: "SECRET_TO".into(),
        };
        assert_eq!(format!("{c:?}"), "ScriptCall::Rename");
    }
}
