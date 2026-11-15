use super::*;

fn raw(payload: &[u8]) -> Vec<u8> {
    let mut out = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
    out.extend_from_slice(payload);
    out
}

fn json_frame(text: &str) -> Vec<u8> {
    raw(text.as_bytes())
}

fn call_frame(seq: u64, name: &str) -> Vec<u8> {
    json_frame(&format!(
        r#"{{"type":"call","seq":{seq},"call":"{name}","args":{{}}}}"#
    ))
}

fn running() -> HelperDecoder {
    let mut d = HelperDecoder::default();
    d.begin_run();
    d
}

fn feed_err(d: &mut HelperDecoder, bytes: &[u8]) -> ProtocolError {
    d.feed(bytes).expect_err("frame must be rejected")
}

#[test]
fn decodes_the_three_helper_frames() {
    let mut d = running();
    let mut bytes = call_frame(1, "ls");
    bytes.extend(json_frame(
        r#"{"type":"output","stream":"stdout","text":"hi"}"#,
    ));
    let frames = d.feed(&bytes).unwrap();
    assert_eq!(
        frames,
        vec![
            HelperFrame::Call {
                seq: 1,
                call: ScriptCall::Ls {
                    selection_only: false
                }
            },
            HelperFrame::Output {
                stream: OutputStream::Stdout,
                text: "hi".into()
            }
        ]
    );
    d.answered(1).unwrap();
    let done = d.feed(&json_frame(r#"{"type":"done"}"#)).unwrap();
    assert_eq!(done, vec![HelperFrame::Done { error: None }]);
    d.finish_eof().unwrap();
}

#[test]
fn byte_at_a_time_equals_whole_buffer() {
    let mut bytes = call_frame(1, "undo_info_query");
    bytes.extend(json_frame(r#"{"type":"done","error":"boom"}"#));
    let whole = {
        let mut d = running();
        let first = d.feed(&bytes[..bytes.len() - 1]).unwrap();
        d.answered(1).unwrap();
        let mut all = first;
        all.extend(d.feed(&bytes[bytes.len() - 1..]).unwrap());
        all
    };
    let mut d = running();
    let mut split = Vec::new();
    for b in &bytes {
        split.extend(d.feed(std::slice::from_ref(b)).unwrap());
        if matches!(split.last(), Some(HelperFrame::Call { .. })) && d.outstanding.is_some() {
            d.answered(1).unwrap();
        }
    }
    assert_eq!(split, whole);
    assert_eq!(whole.len(), 2);
}

#[test]
fn oversize_prefix_is_rejected_before_any_payload() {
    let mut d = running();
    let len = u32::try_from(MAX_HELPER_FRAME_BYTES + 1).unwrap();
    let err = feed_err(&mut d, &len.to_be_bytes());
    assert_eq!(
        err,
        ProtocolError::FrameTooLarge {
            len: MAX_HELPER_FRAME_BYTES + 1,
            max: MAX_HELPER_FRAME_BYTES
        }
    );
    assert!(d.buf.len() <= LEN_PREFIX);
    let mut d = running();
    assert!(matches!(
        feed_err(&mut d, &u32::MAX.to_be_bytes()),
        ProtocolError::FrameTooLarge { .. }
    ));
    let at_limit = format!(
        r#"{{"type":"output","stream":"stderr","text":"{}"}}"#,
        "x".repeat(MAX_HELPER_FRAME_BYTES - 50)
    );
    assert!(at_limit.len() <= MAX_HELPER_FRAME_BYTES);
    assert!(running().feed(&json_frame(&at_limit)).is_ok());
}

#[test]
fn host_frames_encode_with_a_prefix_and_bounds() {
    let decode = |bytes: Vec<u8>| -> Value {
        let len = u32::from_be_bytes(bytes[..LEN_PREFIX].try_into().unwrap()) as usize;
        assert_eq!(len, bytes.len() - LEN_PREFIX);
        serde_json::from_slice(&bytes[LEN_PREFIX..]).unwrap()
    };
    let run = HostFrame::Run {
        code: "print(1)".into(),
        limits: RunLimits::default(),
    };
    let v = decode(encode_host_frame(&run).unwrap());
    assert_eq!(v["type"], "run");
    assert_eq!(v["code"], "print(1)");
    assert_eq!(v["limits"]["wall_ms"], 30_000);
    assert_eq!(v["limits"]["max_calls"], 10_000);
    let ok = HostFrame::Reply {
        seq: 3,
        result: Ok(json!(["a"])),
    };
    assert_eq!(
        decode(encode_host_frame(&ok).unwrap()),
        json!({"type":"reply","seq":3,"ok":["a"]})
    );
    let err = HostFrame::Reply {
        seq: 3,
        result: Err("stale_scene"),
    };
    assert_eq!(
        decode(encode_host_frame(&err).unwrap()),
        json!({"type":"reply","seq":3,"error":"stale_scene"})
    );
    assert_eq!(
        decode(encode_host_frame(&HostFrame::Cancel).unwrap()),
        json!({"type":"cancel"})
    );

    let long = HostFrame::Run {
        code: "x".repeat(MAX_CODE_BYTES + 1),
        limits: RunLimits::default(),
    };
    assert_eq!(encode_host_frame(&long), Err(ProtocolError::InvalidFields));
    let nul = HostFrame::Run {
        code: "a\0b".into(),
        limits: RunLimits::default(),
    };
    assert_eq!(encode_host_frame(&nul), Err(ProtocolError::InvalidFields));
    let huge = HostFrame::Reply {
        seq: 1,
        result: Ok(Value::String("x".repeat(MAX_EDITOR_FRAME_BYTES))),
    };
    assert!(matches!(
        encode_host_frame(&huge),
        Err(ProtocolError::FrameTooLarge { .. })
    ));
}

#[test]
fn debug_prints_the_kind_only() {
    let run = HostFrame::Run {
        code: "SECRET_CODE".into(),
        limits: RunLimits::default(),
    };
    assert_eq!(format!("{run:?}"), "HostFrame::Run");
    let out = HelperFrame::Output {
        stream: OutputStream::Stdout,
        text: "SECRET_OUT".into(),
    };
    assert_eq!(format!("{out:?}"), "HelperFrame::Output");
    let done = HelperFrame::Done {
        error: Some("SECRET_ERR".into()),
    };
    assert_eq!(format!("{done:?}"), "HelperFrame::Done");
}

#[test]
fn protocol_modules_import_no_io_or_process_types() {
    for (name, text) in [
        ("frame", include_str!("../frame.rs")),
        ("protocol", include_str!("../protocol.rs")),
    ] {
        let code = text.split("#[cfg(test)]").next().unwrap();
        for banned in [
            "std::io",
            "std::fs",
            "std::net",
            "std::process",
            "std::time",
            "std::thread",
            "unsafe",
        ] {
            assert!(!code.contains(banned), "{name} uses {banned}");
        }
    }
}

#[test]
fn decoder_debug_hides_buffered_guest_output() {
    let mut d = running();
    let bytes = json_frame(r#"{"type":"output","stream":"stdout","text":"SECRET_PARTIAL"}"#);
    d.feed(&bytes[..bytes.len() - 1]).unwrap();
    let shown = format!("{d:?}");
    assert!(!shown.contains("SECRET"), "{shown}");
    assert!(shown.contains("buffered_bytes"));
}

mod hostile;
