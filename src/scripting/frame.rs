//! Length-prefixed framing and the run state machine for the editor/helper pipes (#57).
//!
//! A frame is a 4-byte big-endian length followed by that many bytes of UTF-8 JSON. Helper to
//! editor frames are hostile input: [`HelperDecoder`] checks the length before buffering, the
//! nesting depth before parsing, and the run rules (one outstanding call, sequential seq,
//! call/output/frame budgets) as each frame completes. The first violation poisons the decoder;
//! the caller then kills the helper.

use super::protocol::{
    MAX_CALLS_PER_RUN, MAX_DONE_ERROR_BYTES, MAX_EDITOR_FRAME_BYTES, MAX_FRAMES_PER_RUN,
    MAX_HELPER_FRAME_BYTES, MAX_JSON_DEPTH, MAX_OUTPUT_BYTES, OutputStream, ProtocolError,
    ScriptCall,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::fmt;

const LEN_PREFIX: usize = 4;
/// Longest script an editor `run` frame may carry (the MCP `run_script` bound).
pub const MAX_CODE_BYTES: usize = 16 * 1024;
const WALL_LIMIT_MS: u64 = 30_000;
const GUEST_MEMORY_BYTES: u64 = 256 * 1024 * 1024;

/// Limits the editor sends with `run`; the helper applies them to the fresh Store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RunLimits {
    pub wall_ms: u64,
    pub guest_memory_bytes: u64,
    pub max_calls: u32,
    pub max_output_bytes: usize,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            wall_ms: WALL_LIMIT_MS,
            guest_memory_bytes: GUEST_MEMORY_BYTES,
            max_calls: MAX_CALLS_PER_RUN,
            max_output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}

/// Editor to helper. Carries no scene token or cursor: those stay in the editor.
#[derive(Clone, PartialEq)]
pub enum HostFrame {
    Run {
        code: String,
        limits: RunLimits,
    },
    /// The answer to call `seq`: a JSON value, or a typed error code.
    Reply {
        seq: u64,
        result: Result<Value, &'static str>,
    },
    Cancel,
}

impl fmt::Debug for HostFrame {
    /// Script text and scene replies are user data, so only the kind is printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Run { .. } => "HostFrame::Run",
            Self::Reply { .. } => "HostFrame::Reply",
            Self::Cancel => "HostFrame::Cancel",
        })
    }
}

/// Serialises one editor frame, refusing a script that is too long or has a NUL and a payload
/// above [`MAX_EDITOR_FRAME_BYTES`].
pub fn encode_host_frame(frame: &HostFrame) -> Result<Vec<u8>, ProtocolError> {
    let value = match frame {
        HostFrame::Run { code, limits } => {
            if code.len() > MAX_CODE_BYTES || code.contains('\0') {
                return Err(ProtocolError::InvalidFields);
            }
            json!({"type": "run", "code": code, "limits": limits})
        }
        HostFrame::Reply {
            seq,
            result: Ok(value),
        } => json!({"type": "reply", "seq": seq, "ok": value}),
        HostFrame::Reply {
            seq,
            result: Err(code),
        } => json!({"type": "reply", "seq": seq, "error": code}),
        HostFrame::Cancel => json!({"type": "cancel"}),
    };
    let payload = serde_json::to_vec(&value).map_err(|_| ProtocolError::InvalidFields)?;
    let too_large = ProtocolError::FrameTooLarge {
        len: payload.len(),
        max: MAX_EDITOR_FRAME_BYTES,
    };
    if payload.len() > MAX_EDITOR_FRAME_BYTES {
        return Err(too_large);
    }
    let len = u32::try_from(payload.len()).map_err(|_| too_large)?;
    let mut out = Vec::with_capacity(LEN_PREFIX + payload.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Helper to editor. Hostile: nothing here is trusted beyond its validated shape.
#[derive(Clone, PartialEq)]
pub enum HelperFrame {
    Call {
        seq: u64,
        call: ScriptCall,
    },
    Output {
        stream: OutputStream,
        text: String,
    },
    /// End of run; `error` is helper-authored text of at most [`MAX_DONE_ERROR_BYTES`].
    Done {
        error: Option<String>,
    },
}

impl fmt::Debug for HelperFrame {
    /// Output and error text come from the guest, so only the kind is printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Call { .. } => "HelperFrame::Call",
            Self::Output { .. } => "HelperFrame::Output",
            Self::Done { .. } => "HelperFrame::Done",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    BeforeRun,
    Running,
    Finished,
}

/// Incremental decoder for one run of one helper (a fresh helper per run, so a fresh decoder).
pub struct HelperDecoder {
    buf: Vec<u8>,
    phase: Phase,
    next_seq: u64,
    outstanding: Option<u64>,
    calls: u32,
    frames: u32,
    output_bytes: usize,
    failed: Option<ProtocolError>,
}

impl fmt::Debug for HelperDecoder {
    /// `buf` holds partial guest output, so only the phase and counters are printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HelperDecoder")
            .field("phase", &self.phase)
            .field("buffered_bytes", &self.buf.len())
            .field("calls", &self.calls)
            .field("frames", &self.frames)
            .field("output_bytes", &self.output_bytes)
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

impl Default for HelperDecoder {
    fn default() -> Self {
        Self {
            buf: Vec::new(),
            phase: Phase::BeforeRun,
            next_seq: 1,
            outstanding: None,
            calls: 0,
            frames: 0,
            output_bytes: 0,
            failed: None,
        }
    }
}

impl HelperDecoder {
    /// Call when the `run` frame is sent; helper frames before this are [`ProtocolError::OutsideRun`].
    pub fn begin_run(&mut self) {
        if self.phase == Phase::BeforeRun {
            self.phase = Phase::Running;
        }
    }

    /// Call when the reply to call `seq` is sent; anything else is [`ProtocolError::UnexpectedReply`].
    pub fn answered(&mut self, seq: u64) -> Result<(), ProtocolError> {
        if self.outstanding == Some(seq) {
            self.outstanding = None;
            Ok(())
        } else {
            Err(ProtocolError::UnexpectedReply)
        }
    }

    /// Consumes bytes read from the helper and returns the frames they complete. Memory held is
    /// at most one frame (the prefix is checked before any payload is buffered). After an error
    /// every later call returns that error.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<HelperFrame>, ProtocolError> {
        if let Some(err) = self.failed {
            return Err(err);
        }
        let mut frames = Vec::new();
        self.feed_inner(chunk, &mut frames)
            .inspect_err(|&err| self.failed = Some(err))?;
        Ok(frames)
    }

    /// Call at end of stream: a partial frame is [`ProtocolError::Truncated`].
    pub fn finish_eof(&mut self) -> Result<(), ProtocolError> {
        if let Some(err) = self.failed {
            return Err(err);
        }
        if self.buf.is_empty() {
            return Ok(());
        }
        self.failed = Some(ProtocolError::Truncated);
        Err(ProtocolError::Truncated)
    }

    fn feed_inner(
        &mut self,
        mut chunk: &[u8],
        frames: &mut Vec<HelperFrame>,
    ) -> Result<(), ProtocolError> {
        while !chunk.is_empty() {
            let end = match self.payload_len()? {
                None => LEN_PREFIX,
                Some(len) => LEN_PREFIX + len,
            };
            let take = (end - self.buf.len()).min(chunk.len());
            self.buf.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
            // The prefix may have just completed: bound it before any payload is buffered.
            let end = match self.payload_len()? {
                None => continue,
                Some(len) => LEN_PREFIX + len,
            };
            if self.buf.len() == end {
                let framed = std::mem::take(&mut self.buf);
                frames.push(self.decode(&framed[LEN_PREFIX..])?);
            }
        }
        Ok(())
    }

    /// `None` until the 4-byte prefix is buffered; then its checked value.
    fn payload_len(&self) -> Result<Option<usize>, ProtocolError> {
        let Some(prefix) = self.buf.get(..LEN_PREFIX) else {
            return Ok(None);
        };
        let prefix: [u8; LEN_PREFIX] = prefix.try_into().map_err(|_| ProtocolError::Truncated)?;
        let len = usize::try_from(u32::from_be_bytes(prefix)).unwrap_or(usize::MAX);
        match len {
            0 => Err(ProtocolError::EmptyFrame),
            n if n > MAX_HELPER_FRAME_BYTES => Err(ProtocolError::FrameTooLarge {
                len: n,
                max: MAX_HELPER_FRAME_BYTES,
            }),
            n => Ok(Some(n)),
        }
    }

    fn decode(&mut self, payload: &[u8]) -> Result<HelperFrame, ProtocolError> {
        if self.phase != Phase::Running {
            return Err(ProtocolError::OutsideRun);
        }
        if self.frames >= MAX_FRAMES_PER_RUN {
            return Err(ProtocolError::FrameLimit);
        }
        self.frames += 1;
        let text = std::str::from_utf8(payload).map_err(|_| ProtocolError::NotUtf8)?;
        if exceeds_depth(payload) {
            return Err(ProtocolError::TooDeep);
        }
        let Value::Object(mut map) =
            serde_json::from_str(text).map_err(|_| ProtocolError::NotJson)?
        else {
            return Err(ProtocolError::NotAnObject);
        };
        let kind = match map.remove("type") {
            Some(Value::String(kind)) => kind,
            _ => return Err(ProtocolError::MissingType),
        };
        match kind.as_str() {
            "call" => self.call(map),
            "output" => self.output(map),
            "done" => self.done(map),
            _ => Err(ProtocolError::UnknownFrameType),
        }
    }

    fn call(&mut self, mut map: Map<String, Value>) -> Result<HelperFrame, ProtocolError> {
        if self.outstanding.is_some() {
            return Err(ProtocolError::SecondOutstandingCall);
        }
        let seq = map
            .remove("seq")
            .and_then(|v| v.as_u64())
            .filter(|seq| *seq == self.next_seq)
            .ok_or(ProtocolError::InvalidSeq)?;
        if self.calls >= MAX_CALLS_PER_RUN {
            return Err(ProtocolError::CallLimit);
        }
        let Some(Value::String(name)) = map.remove("call") else {
            return Err(ProtocolError::InvalidFields);
        };
        let args = map.remove("args").unwrap_or_else(|| json!({}));
        no_extra_fields(&map)?;
        let call = ScriptCall::from_wire(&name, args)?;
        self.calls += 1;
        self.next_seq = seq + 1;
        self.outstanding = Some(seq);
        Ok(HelperFrame::Call { seq, call })
    }

    fn output(&mut self, mut map: Map<String, Value>) -> Result<HelperFrame, ProtocolError> {
        let stream = map
            .remove("stream")
            .and_then(|v| serde_json::from_value(v).ok())
            .ok_or(ProtocolError::InvalidFields)?;
        let Some(Value::String(text)) = map.remove("text") else {
            return Err(ProtocolError::InvalidFields);
        };
        no_extra_fields(&map)?;
        let total = self.output_bytes.saturating_add(text.len());
        if total > MAX_OUTPUT_BYTES {
            return Err(ProtocolError::OutputLimit);
        }
        self.output_bytes = total;
        Ok(HelperFrame::Output { stream, text })
    }

    fn done(&mut self, mut map: Map<String, Value>) -> Result<HelperFrame, ProtocolError> {
        let error = match map.remove("error") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) if text.len() <= MAX_DONE_ERROR_BYTES => Some(text),
            Some(_) => return Err(ProtocolError::InvalidFields),
        };
        no_extra_fields(&map)?;
        if self.outstanding.is_some() {
            return Err(ProtocolError::DoneWithOutstandingCall);
        }
        self.phase = Phase::Finished;
        Ok(HelperFrame::Done { error })
    }
}

fn no_extra_fields(map: &Map<String, Value>) -> Result<(), ProtocolError> {
    if map.is_empty() {
        Ok(())
    } else {
        Err(ProtocolError::InvalidFields)
    }
}

/// True when `[`/`{` nesting outside strings exceeds [`MAX_JSON_DEPTH`]; checked before parsing
/// so a hostile frame cannot drive the parser's recursion.
fn exceeds_depth(bytes: &[u8]) -> bool {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for &b in bytes {
        if in_string {
            match (escaped, b) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > MAX_JSON_DEPTH {
                    return true;
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}
