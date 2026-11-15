use super::*;

#[test]
fn malformed_frames_give_typed_errors() {
    let deep = format!("{}{}", "[".repeat(30_000), "]".repeat(30_000));
    let deep_obj = format!(
        r#"{{"type":"done","x":{}{}}}"#,
        "[".repeat(MAX_JSON_DEPTH),
        "]".repeat(MAX_JSON_DEPTH)
    );
    let cases: Vec<(Vec<u8>, ProtocolError)> = vec![
        (raw(b""), ProtocolError::EmptyFrame),
        (raw(&[0xff, 0xfe]), ProtocolError::NotUtf8),
        (raw(b"{nope"), ProtocolError::NotJson),
        (raw(b"[1]"), ProtocolError::NotAnObject),
        (raw(b"null"), ProtocolError::NotAnObject),
        (raw(deep.as_bytes()), ProtocolError::TooDeep),
        (raw(deep_obj.as_bytes()), ProtocolError::TooDeep),
        (json_frame("{}"), ProtocolError::MissingType),
        (json_frame(r#"{"type":5}"#), ProtocolError::MissingType),
        (
            json_frame(r#"{"type":"reply"}"#),
            ProtocolError::UnknownFrameType,
        ),
        (
            json_frame(r#"{"type":"run"}"#),
            ProtocolError::UnknownFrameType,
        ),
        (
            json_frame(r#"{"type":"call","seq":1,"call":"eval"}"#),
            ProtocolError::UnknownCall,
        ),
        (
            json_frame(r#"{"type":"call","seq":1,"call":"ls","extra":1}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"call","seq":1}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"call","seq":1,"call":7}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"call","call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"call","seq":0,"call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"call","seq":2,"call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"call","seq":-1,"call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"call","seq":1.0,"call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"call","seq":"1","call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"call","seq":18446744073709551616,"call":"ls"}"#),
            ProtocolError::InvalidSeq,
        ),
        (
            json_frame(r#"{"type":"output","stream":"stdin","text":""}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"output","stream":"stdout"}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"output","stream":"stdout","text":1}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"output","stream":"stdout","text":"","x":0}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"done","error":5}"#),
            ProtocolError::InvalidFields,
        ),
        (
            json_frame(r#"{"type":"done","ok":true}"#),
            ProtocolError::InvalidFields,
        ),
    ];
    for (bytes, want) in cases {
        assert_eq!(
            feed_err(&mut running(), &bytes),
            want,
            "{:?}",
            String::from_utf8_lossy(&bytes[4..bytes.len().min(80)])
        );
    }
    let long = format!(
        r#"{{"type":"done","error":"{}"}}"#,
        "e".repeat(MAX_DONE_ERROR_BYTES + 1)
    );
    assert_eq!(
        feed_err(&mut running(), &json_frame(&long)),
        ProtocolError::InvalidFields
    );
    let at = format!(
        r#"{{"type":"done","error":"{}"}}"#,
        "e".repeat(MAX_DONE_ERROR_BYTES)
    );
    assert!(running().feed(&json_frame(&at)).is_ok());
}

#[test]
fn run_rules_are_enforced() {
    // Before the run starts, and after done.
    assert_eq!(
        feed_err(&mut HelperDecoder::default(), &call_frame(1, "ls")),
        ProtocolError::OutsideRun
    );
    let mut d = running();
    d.feed(&json_frame(r#"{"type":"done"}"#)).unwrap();
    assert_eq!(
        feed_err(&mut d, &call_frame(1, "ls")),
        ProtocolError::OutsideRun
    );
    // A second call while one is unanswered, in one read or across reads.
    let mut two = call_frame(1, "ls");
    two.extend(call_frame(2, "ls"));
    assert_eq!(
        feed_err(&mut running(), &two),
        ProtocolError::SecondOutstandingCall
    );
    let mut d = running();
    d.feed(&call_frame(1, "ls")).unwrap();
    assert_eq!(
        feed_err(&mut d, &call_frame(2, "ls")),
        ProtocolError::SecondOutstandingCall
    );
    // done with an unanswered call.
    let mut d = running();
    d.feed(&call_frame(1, "ls")).unwrap();
    assert_eq!(
        feed_err(&mut d, &json_frame(r#"{"type":"done"}"#)),
        ProtocolError::DoneWithOutstandingCall
    );
    // seq reuse and skipping after a proper answer.
    let mut d = running();
    d.feed(&call_frame(1, "ls")).unwrap();
    d.answered(1).unwrap();
    assert_eq!(
        feed_err(&mut d, &call_frame(1, "ls")),
        ProtocolError::InvalidSeq
    );
    let mut d = running();
    d.feed(&call_frame(1, "ls")).unwrap();
    d.answered(1).unwrap();
    assert_eq!(
        feed_err(&mut d, &call_frame(3, "ls")),
        ProtocolError::InvalidSeq
    );
    // Replies must name the outstanding call.
    let mut d = running();
    assert_eq!(d.answered(1), Err(ProtocolError::UnexpectedReply));
    d.feed(&call_frame(1, "ls")).unwrap();
    assert_eq!(d.answered(2), Err(ProtocolError::UnexpectedReply));
    d.answered(1).unwrap();
    assert_eq!(d.answered(1), Err(ProtocolError::UnexpectedReply));
}

#[test]
fn budgets_stop_the_run() {
    let mut d = running();
    for seq in 1..=u64::from(MAX_CALLS_PER_RUN) {
        d.feed(&call_frame(seq, "ls")).unwrap();
        d.answered(seq).unwrap();
    }
    let over = u64::from(MAX_CALLS_PER_RUN) + 1;
    assert_eq!(
        feed_err(&mut d, &call_frame(over, "ls")),
        ProtocolError::CallLimit
    );

    let mut d = running();
    let chunk = "x".repeat(MAX_HELPER_FRAME_BYTES / 2);
    let out = json_frame(&format!(
        r#"{{"type":"output","stream":"stdout","text":"{chunk}"}}"#
    ));
    for _ in 0..MAX_OUTPUT_BYTES / chunk.len() {
        d.feed(&out).unwrap();
    }
    let one = json_frame(r#"{"type":"output","stream":"stdout","text":"yy"}"#);
    assert_eq!(feed_err(&mut d, &one), ProtocolError::OutputLimit);

    let mut d = running();
    let empty = json_frame(r#"{"type":"output","stream":"stderr","text":""}"#);
    for _ in 0..MAX_FRAMES_PER_RUN {
        d.feed(&empty).unwrap();
    }
    assert_eq!(feed_err(&mut d, &empty), ProtocolError::FrameLimit);
}

#[test]
fn truncation_and_poisoning() {
    let bytes = call_frame(1, "ls");
    for cut in 1..bytes.len() {
        let mut d = running();
        assert!(d.feed(&bytes[..cut]).unwrap().is_empty());
        assert_eq!(d.finish_eof(), Err(ProtocolError::Truncated), "cut {cut}");
    }
    assert_eq!(running().finish_eof(), Ok(()));
    let mut d = running();
    assert_eq!(feed_err(&mut d, &raw(b"[]")), ProtocolError::NotAnObject);
    // The decoder stays failed, even for a valid frame.
    assert_eq!(
        feed_err(&mut d, &call_frame(1, "ls")),
        ProtocolError::NotAnObject
    );
    assert_eq!(d.finish_eof(), Err(ProtocolError::NotAnObject));
}

#[test]
fn arbitrary_bytes_never_panic() {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as u32
    };
    let templates: [&[u8]; 4] = [
        br#"{"type":"call","seq":1,"call":"xform","args":{"name":"n","translate":[1,2,3]}}"#,
        br#"{"type":"output","stream":"stdout","text":"a\u00e9\ud800"}"#,
        br#"{"type":"done","error":null}"#,
        br#"[[[{"type":"#,
    ];
    for round in 0..2_000 {
        let mut d = running();
        let mut payload = templates[round % templates.len()].to_vec();
        for _ in 0..(next() % 4) {
            let at = next() as usize % payload.len();
            payload[at] = next() as u8;
        }
        let mut bytes = if next() % 3 == 0 {
            (0..next() % 64).map(|_| next() as u8).collect::<Vec<_>>()
        } else {
            raw(&payload)
        };
        bytes.truncate(next() as usize % (bytes.len() + 1));
        let step = 1 + next() as usize % 9;
        for piece in bytes.chunks(step) {
            if d.feed(piece).is_err() {
                break;
            }
        }
        let _ = d.finish_eof();
    }
}

#[test]
fn depth_scan_counts_brackets_outside_strings_at_the_exact_bound() {
    let brackets = "[".repeat(MAX_JSON_DEPTH + 2) + &"{".repeat(MAX_JSON_DEPTH);
    let quoted = format!(r#"{{"type":"output","stream":"stdout","text":"\"{brackets}"}}"#);
    let frames = running().feed(&json_frame(&quoted)).unwrap();
    assert_eq!(frames.len(), 1);
    // The frame object is depth 1; `x` is an unknown field, so a pass is InvalidFields.
    let nested = |extra: usize| {
        let open = "[".repeat(MAX_JSON_DEPTH - 1 + extra);
        let close = "]".repeat(MAX_JSON_DEPTH - 1 + extra);
        json_frame(&format!(r#"{{"type":"done","x":{open}{close}}}"#))
    };
    assert_eq!(
        feed_err(&mut running(), &nested(0)),
        ProtocolError::InvalidFields
    );
    assert_eq!(feed_err(&mut running(), &nested(1)), ProtocolError::TooDeep);
}
