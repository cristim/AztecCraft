use super::*;
use crate::maya_ascii::{ErrorKind, MAX_WORK};
use std::mem::size_of;

fn parse(body: &str) -> Records {
    inspect(Arc::from(
        format!("//Maya ASCII 2016 scene\n{body}").into_bytes(),
    ))
    .unwrap()
}

fn partition(records: &Records) {
    let mut end = 0;
    let mut token_end = 0;
    for statement in &records.statements {
        assert_eq!(statement.retained.start, end);
        assert!(statement.retained.start <= statement.command.start);
        assert_eq!(statement.command.end, statement.retained.end);
        assert_eq!(records.source[statement.command.end - 1], b';');
        assert_eq!(statement.tokens.start, token_end);
        assert!(statement.tokens.start < statement.tokens.end);
        for token in &records.tokens[statement.tokens.clone()] {
            assert!(token.span.start >= statement.command.start);
            assert!(token.span.end < statement.command.end);
        }
        end = statement.retained.end;
        token_end = statement.tokens.end;
    }
    assert_eq!(records.trailing, end..records.source.len());
    assert_eq!(token_end, records.tokens.len());
}

#[test]
fn escaped_and_multiline_strings_are_inert_and_preserve_original_source() {
    let body = "createNode script -n \"script1\";\nsetAttr \".b\" -type \"string\" (\"a; \\\"quoted\\\" \\n \\t \\\\ `cmd`; //\"+\"/* body */ $value;\");\nsetAttr \".st\" 3; // retained end";
    let bytes: Arc<[u8]> = Arc::from(format!("//Maya ASCII 2016 scene\n{body}").into_bytes());
    let mut records = inspect(bytes.clone()).unwrap();
    assert!(Arc::ptr_eq(&bytes, &records.source));
    assert_eq!(records.statements.len(), 3);
    let tokens = &records.tokens[records.statements[1].tokens.clone()];
    assert_eq!(
        tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
        [
            TokenKind::Word,
            TokenKind::Quoted,
            TokenKind::Word,
            TokenKind::Quoted,
            TokenKind::LeftParen,
            TokenKind::Quoted,
            TokenKind::Plus,
            TokenKind::Quoted,
            TokenKind::RightParen,
        ]
    );
    let text = tokens[5].text(&records.source, &mut records.work).unwrap();
    assert!(text.contains("`cmd`; //"));
    assert!(text.contains("\\n"));
    assert_eq!(records.source.as_ref(), bytes.as_ref());
    partition(&records);
    let multiline = parse("setAttr \".b\" -type \"string\" \"line1\nline2\tend\";");
    assert_eq!(multiline.statements.len(), 1);
}

#[test]
fn comments_and_trailing_trivia_have_a_gap_free_partition() {
    let records = parse(
        "/* ; \" */ createNode transform -n \"a\";\r\n// ; ignored\nsetAttr \".t\" -type \"double3\" +1 2e+3 -4e-2; /* end */",
    );
    assert_eq!(records.statements.len(), 2);
    assert_eq!(records.tokens.len(), 11);
    partition(&records);
    let header_only = parse("// final line comment");
    assert!(header_only.statements.is_empty());
    partition(&header_only);
    assert!(inspect(Arc::from(&b"//Maya ASCII 2026 scene\r\n"[..])).is_ok());
}

#[test]
fn truncated_empty_and_executable_syntax_refuse_without_private_payloads() {
    for body in [
        ";",
        "cmd;;",
        "cmd",
        "cmd \"unterminated",
        "cmd /* missing",
        "cmd (\"a\"",
        "cmd ();",
        "cmd (\"a\"+);",
        "cmd \"a\"\"b\";",
    ] {
        let error = inspect(Arc::from(
            format!("//Maya ASCII 2016 scene\n{body}").into_bytes(),
        ))
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Malformed, "{body}");
    }
    for body in [
        "cmd `call`;",
        "cmd $value;",
        "cmd (1+2);",
        "cmd ((\"a\"));",
        "cmd 1+2;",
        "cmd 1*2;",
        "cmd \"bad\\q\";",
        "cmd { call; };",
        "cmd = 2;",
        "cmd \"bad\0\";",
    ] {
        let error = inspect(Arc::from(
            format!("//Maya ASCII 2016 scene\n{body}").into_bytes(),
        ))
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unsupported, "{body}");
    }
    let marker = "PRIVATE_SCRIPT_MARKER";
    let records = parse(&format!("cmd \"{marker}\";"));
    let debug = format!("{records:?}");
    assert!(!debug.contains(marker));
    assert!(!debug.contains("80, 82, 73, 86, 65, 84, 69"));
    let error = inspect(Arc::from(
        format!("//Maya ASCII 2016 scene\n{marker} `bad`;").into_bytes(),
    ))
    .unwrap_err();
    assert!(!format!("{error:?} {error}").contains(marker));
}

#[test]
fn invalid_utf8_header_and_source_ranges_are_checked() {
    for bytes in [
        &b""[..],
        &b"// other\ncmd;"[..],
        &b"//Maya ASCII  scene\n"[..],
        &b"//Maya ASCII . scene\n"[..],
        &b"//Maya ASCII 2016 scene\ncmd \xff;"[..],
    ] {
        assert_eq!(
            inspect(Arc::from(bytes)).unwrap_err().kind,
            ErrorKind::Malformed
        );
    }
    let mut records = parse("cmd \"caf\u{e9}\";");
    let token = &records.tokens[1];
    assert_eq!(
        token.text(&records.source, &mut records.work).unwrap(),
        "\"caf\u{e9}\""
    );
    let invalid = Token {
        kind: TokenKind::Word,
        span: 0..usize::MAX,
    };
    assert_eq!(
        invalid
            .text(&records.source, &mut records.work)
            .unwrap_err()
            .kind,
        ErrorKind::Malformed
    );
    let invalid = Token {
        kind: TokenKind::Word,
        span: token.span.end - 2..token.span.end,
    };
    assert_eq!(
        invalid
            .text(&records.source, &mut records.work)
            .unwrap_err()
            .kind,
        ErrorKind::Malformed
    );
}

#[test]
fn shared_capacity_and_work_meters_continue_after_lexing() {
    let mut records = parse("createNode transform -n \"shape\"; setAttr \".v\" yes;");
    let cost = records.source.len()
        + records.statements.capacity() * size_of::<Statement>()
        + records.tokens.capacity() * size_of::<Token>();
    records.budget.charge(128 * 1024 * 1024 - cost).unwrap();
    assert!(records.budget.charge(1).is_err());
    assert!(records.work.used >= records.source.len() * 3);
    records.work.charge(MAX_WORK - records.work.used).unwrap();
    assert_eq!(
        records.work.charge(1).unwrap_err().kind,
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        records.work.charge(usize::MAX).unwrap_err().kind,
        ErrorKind::ResourceLimit
    );
    records.work.charge(0).unwrap();
    assert_eq!(
        records.tokens[0]
            .text(&records.source, &mut records.work)
            .unwrap_err()
            .kind,
        ErrorKind::ResourceLimit
    );
    let bytes: Arc<[u8]> = Arc::from(vec![b' '; MAX_BYTES + 1]);
    assert_eq!(inspect(bytes).unwrap_err().kind, ErrorKind::ResourceLimit);
}

#[test]
fn exact_token_and_statement_caps_are_checked_by_the_real_scan() {
    let mut work = Work::new();
    let counts = scanner(b"x;", &mut work);
    assert_eq!(counts.counts.tokens, 0);
    let mut scan = scanner(b"x;", &mut work);
    scan.counts.tokens = MAX_TOKENS - 1;
    scan.first_token = MAX_TOKENS - 1;
    let (counts, _) = scan.run(|_| Ok(())).unwrap();
    assert_eq!(counts.tokens, MAX_TOKENS);
    let mut scan = scanner(b"x;", &mut work);
    scan.counts.tokens = MAX_TOKENS;
    assert_eq!(
        scan.run(|_| Ok(())).unwrap_err().kind,
        ErrorKind::ResourceLimit
    );
    let mut scan = scanner(b"x;", &mut work);
    scan.counts.statements = MAX_STATEMENTS - 1;
    let (counts, _) = scan.run(|_| Ok(())).unwrap();
    assert_eq!(counts.statements, MAX_STATEMENTS);
    let mut scan = scanner(b"x;", &mut work);
    scan.counts.statements = MAX_STATEMENTS;
    assert_eq!(
        scan.run(|_| Ok(())).unwrap_err().kind,
        ErrorKind::ResourceLimit
    );
}

#[test]
#[ignore = "requires explicit pinned genuine MA path and separate SHA256/blob precheck"]
fn private_genuine_ma_keeps_the_entire_ui_script_in_one_statement() {
    use std::{fs::OpenOptions, io::Read};
    let path = std::env::var_os("AZTECCRAFT_MA_CURVE_FIXTURE")
        .expect("explicit pinned MA fixture path is required");
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .expect("cannot open explicit regular MA fixture");
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 82_189);
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes.len(), 82_189);
    let source: Arc<[u8]> = Arc::from(bytes);
    let mut records = inspect(source.clone()).unwrap();
    assert!(Arc::ptr_eq(&source, &records.source));
    assert_eq!(records.statements.len(), 130);
    let script = records
        .statements
        .iter()
        .find(|statement| statement.command.start == 4079)
        .unwrap();
    assert_eq!(script.command, 4079..80288);
    let next = records
        .statements
        .iter()
        .find(|statement| statement.command.start == 80290)
        .unwrap();
    assert_eq!(next.command, 80290..80306);
    assert_eq!(
        records.tokens[script.tokens.start]
            .text(&records.source, &mut records.work)
            .unwrap(),
        "setAttr"
    );
    assert_eq!(
        records.tokens[script.tokens.end - 1].kind,
        TokenKind::RightParen
    );
    partition(&records);
    assert!(source.as_ref() == records.source.as_ref());
}
