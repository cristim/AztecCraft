use super::*;

fn parse_text(text: &str) -> Result<ParsedObj, ObjImportError> {
    parse(text.as_bytes())
}

const TRI: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

#[test]
fn limits_are_pinned_to_their_provisional_values() {
    assert_eq!(MAX_FILE_BYTES, 32 * 1024 * 1024);
    assert_eq!(MAX_LINE_BYTES, 64 * 1024);
    assert_eq!(MAX_VERTICES, 262_144);
    assert_eq!(MAX_FACES, 262_144);
    assert_eq!(MAX_CORNERS, 1_048_576);
    assert_eq!(MAX_CORNERS_PER_FACE, 1024);
    assert_eq!(MAX_OBJECTS, 1024);
    assert_eq!(MAX_NAME_BYTES, 256);
    assert_eq!(MAX_COORD, 1e12);
}

#[test]
fn a_triangle_parses_into_one_unnamed_object() {
    let parsed = parse_text(TRI).unwrap();
    assert_eq!(
        parsed.vertices,
        [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
    assert_eq!(parsed.objects.len(), 1);
    assert_eq!(parsed.objects[0].name, "");
    assert_eq!(parsed.objects[0].faces, [vec![0, 1, 2]]);
    assert_eq!(parsed.ignored_records, 0);
}

#[test]
fn an_empty_file_has_no_faces() {
    assert_eq!(parse_text(""), Err(ObjImportError::NoFaces));
}

fn err(text: &str) -> ObjImportError {
    parse_text(text).unwrap_err()
}

fn with_tri(extra: &str) -> String {
    format!("{TRI}{extra}")
}

fn vertices(count: usize) -> String {
    "v 0 0 0\n".repeat(count)
}

#[test]
fn every_corner_form_and_index_style_resolves() {
    let head = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvt 0 0\nvn 0 0 1\n";
    for face in [
        "f 1 2 3",
        "f 1/1 2/1 3/1",
        "f 1//1 2//1 3//1",
        "f 1/1/1 2/1/1 3/1/1",
        "f -4 -3 -2",
        "f 01 02 03",
        "f -1/-1/-1 -2/-1/-1 -3/-1/-1",
    ] {
        let parsed = parse_text(&format!("{head}{face}\n")).unwrap();
        assert_eq!(parsed.objects[0].faces.len(), 1, "{face}");
    }
    let quad = parse_text(&format!("{head}f -1 -2 -3 -4\n")).unwrap();
    assert_eq!(quad.objects[0].faces, [vec![3, 2, 1, 0]]);
    let ngon = format!("{}f 1 2 3 4 5 6 7\n", vertices(7).replace("0 0 0", "1 2 3"));
    assert_eq!(parse_text(&ngon).unwrap().objects[0].faces[0].len(), 7);
}

#[test]
fn groups_comments_line_endings_and_continuations() {
    let text = "\u{feff}# c\r\nv 0 0 0 1\r\nv 1 0 0 0.5 0.5 0.5 # inline\r\nv 0 1 0\r\n\r\n\
                f 1 2 3\r\no first\r\nf 1 2 \\\r\n3\r\ng second\r\nf 3 2 1\r\ng\r\no empty\r\n";
    let parsed = parse_text(text).unwrap();
    let names: Vec<_> = parsed.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["", "first", "second"]);
    assert_eq!(parsed.objects[1].faces, [vec![0, 1, 2]]);
}

#[test]
fn numbers_follow_the_documented_rules() {
    let first = |x: &str| parse_text(&format!("v {x} 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"));
    for ok in ["1e2", "-0", ".5", "5.", "+5", "1e-320", "1e12", "-1E+2"] {
        assert!(first(ok).is_ok(), "{ok}");
    }
    assert!(first("-0").unwrap().vertices[0][0].is_sign_negative());
    assert_eq!(first("1e-320").unwrap().vertices[0][0], 0.0);
    for out in [
        "1e309",
        "nan",
        "inf",
        "-inf",
        "3.5e38",
        "1.0000001e12",
        "-1e13",
    ] {
        assert!(
            matches!(
                first(out),
                Err(ObjImportError::CoordinateOutOfRange { line: 1 })
            ),
            "{out}"
        );
    }
    for bad in ["0x10", "abc", "1,5", "--1"] {
        assert!(
            matches!(
                first(bad),
                Err(ObjImportError::Syntax {
                    line: 1,
                    what: SyntaxKind::BadNumber
                })
            ),
            "{bad}"
        );
    }
    assert_eq!(
        err("v 1 2\n"),
        ObjImportError::Syntax {
            line: 1,
            what: SyntaxKind::VertexNeedsThreeNumbers
        }
    );
}

#[test]
fn index_errors_carry_their_line() {
    let face = |f: &str| err(&with_tri(&format!("vt 0 0\nvn 0 0 1\n{f}\n")));
    for out in [
        "f 0 1 2",
        "f 1 2 4",
        "f -4 1 2",
        "f 9223372036854775807 1 2",
        "f -9223372036854775808 1 2",
        "f 99999999999999999999 1 2",
        "f 1/2 2 3",
        "f 1//2 2 3",
        "f 1/1/2 2 3",
    ] {
        assert_eq!(
            face(out),
            ObjImportError::IndexOutOfRange { line: 7 },
            "{out}"
        );
    }
    for bad in [
        "f 1/ 2 3",
        "f 1// 2 3",
        "f /1 2 3",
        "f 1/2/3/4 2 3",
        "f 1//x 2 3",
        "f +1 2 3",
        "f a 2 3",
    ] {
        assert_eq!(
            face(bad),
            ObjImportError::Syntax {
                line: 7,
                what: SyntaxKind::BadFaceCorner
            },
            "{bad}"
        );
    }
    assert_eq!(face("f 1 2"), ObjImportError::DegenerateFace { line: 7 });
    assert_eq!(face("f 1 2 1"), ObjImportError::DegenerateFace { line: 7 });
    assert_eq!(
        face("f -1 3 -1"),
        ObjImportError::DegenerateFace { line: 7 }
    );
    assert_eq!(
        err("v 0 0 0\nf 1 1 1\n"),
        ObjImportError::DegenerateFace { line: 2 }
    );
}

#[test]
fn forward_references_are_refused() {
    assert_eq!(
        err("v 0 0 0\nf 1 2 3\nv 1 0 0\nv 0 1 0\n"),
        ObjImportError::IndexOutOfRange { line: 2 }
    );
}

#[test]
fn unsupported_records_are_refused_and_dropped_ones_counted() {
    for keyword in [
        "cstype", "deg", "curv", "curv2", "surf", "parm", "trim", "hole", "scrv", "sp", "end",
        "con", "step", "bmat",
    ] {
        assert_eq!(
            err(&with_tri(&format!("{keyword} 1\n"))),
            ObjImportError::Unsupported {
                line: 5,
                what: UnsupportedKind::FreeForm
            },
            "{keyword}"
        );
    }
    for keyword in ["foo", "V", "vw"] {
        assert_eq!(
            err(&with_tri(&format!("{keyword} 1\n"))),
            ObjImportError::Unsupported {
                line: 5,
                what: UnsupportedKind::UnknownKeyword
            }
        );
    }
    assert_eq!(
        err(&with_tri("g a b\n")),
        ObjImportError::Unsupported {
            line: 5,
            what: UnsupportedKind::OverlappingGroups
        }
    );
    let dropped = "vn 0 0 1\nvt 0 0\nvp 0\nusemtl m\nmtllib a.mtl\ns off\nl 1 2\np 1\nmg 1\nmaplib a\n\
                   usemap a\nbevel on\nc_interp on\nd_interp on\nlod 1\nshadow_obj a\ntrace_obj a\nctech a\nstech a\n";
    assert_eq!(parse_text(&with_tri(dropped)).unwrap().ignored_records, 19);
}

#[test]
fn files_without_a_face_are_refused() {
    for text in [
        "",
        "  \n\t\n",
        "# only\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\n",
        "o a\ng b\n",
    ] {
        assert_eq!(err(text), ObjImportError::NoFaces, "{text:?}");
    }
}

#[test]
fn invalid_utf8_is_refused_with_its_line() {
    assert_eq!(
        parse(&b"v 0 0 0\n# \xff\n"[..]),
        Err(ObjImportError::InvalidUtf8 { line: 2 })
    );
}

#[test]
fn limits_hold_exactly_at_the_boundary() {
    let tri_faces = |faces: usize| format!("{}{}", vertices(3), "f 1 2 3\n".repeat(faces));
    assert!(parse_text(&format!("{}f 1 2 3\n", vertices(MAX_VERTICES))).is_ok());
    assert_eq!(
        err(&vertices(MAX_VERTICES + 1)),
        ObjImportError::LimitExceeded(Limit::Vertices)
    );
    assert!(parse_text(&tri_faces(MAX_FACES)).is_ok());
    assert_eq!(
        err(&tri_faces(MAX_FACES + 1)),
        ObjImportError::LimitExceeded(Limit::Faces)
    );
    let wide = |count: usize| {
        let indices: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
        format!("f {}\n", indices.join(" "))
    };
    let per_face = format!(
        "{}{}",
        vertices(MAX_CORNERS_PER_FACE + 1),
        wide(MAX_CORNERS_PER_FACE)
    );
    assert!(parse_text(&per_face).is_ok());
    let over = format!(
        "{}{}",
        vertices(MAX_CORNERS_PER_FACE + 1),
        wide(MAX_CORNERS_PER_FACE + 1)
    );
    assert_eq!(
        err(&over),
        ObjImportError::LimitExceeded(Limit::CornersPerFace)
    );
    let rows = MAX_CORNERS / MAX_CORNERS_PER_FACE;
    let corners = format!(
        "{}{}",
        vertices(MAX_CORNERS_PER_FACE),
        wide(MAX_CORNERS_PER_FACE).repeat(rows)
    );
    assert!(parse_text(&corners).is_ok());
    assert_eq!(
        err(&format!("{corners}f 1 2 3\n")),
        ObjImportError::LimitExceeded(Limit::Corners)
    );
    let objects =
        |count: usize| (0..count).fold(vertices(3), |text, i| format!("{text}o n{i}\nf 1 2 3\n"));
    assert!(parse_text(&objects(MAX_OBJECTS)).is_ok());
    assert_eq!(
        err(&objects(MAX_OBJECTS + 1)),
        ObjImportError::LimitExceeded(Limit::Objects)
    );
    let named = |len: usize| format!("{TRI}o {}\nf 1 2 3\n", "a".repeat(len));
    assert_eq!(
        parse_text(&named(MAX_NAME_BYTES)).unwrap().objects[1]
            .name
            .len(),
        MAX_NAME_BYTES
    );
    assert_eq!(
        err(&named(MAX_NAME_BYTES + 1)),
        ObjImportError::LimitExceeded(Limit::NameBytes)
    );
}

#[test]
fn line_and_file_size_boundaries() {
    let comment = |len: usize| format!("#{}", "x".repeat(len - 1));
    assert!(parse_text(&format!("{TRI}{}\n", comment(MAX_LINE_BYTES))).is_ok());
    assert!(parse_text(&format!("{TRI}{}", comment(MAX_LINE_BYTES))).is_ok());
    for suffix in ["\n", ""] {
        assert_eq!(
            err(&format!("{TRI}{}{suffix}", comment(MAX_LINE_BYTES + 1))),
            ObjImportError::LineTooLong { line: 5 }
        );
    }
    let mut exact = TRI.as_bytes().to_vec();
    while exact.len() + 1024 <= MAX_FILE_BYTES as usize {
        exact.push(b'#');
        exact.extend(std::iter::repeat_n(b'x', 1022));
        exact.push(b'\n');
    }
    let pad = MAX_FILE_BYTES as usize - exact.len();
    exact.extend(std::iter::repeat_n(b'\n', pad));
    assert_eq!(exact.len() as u64, MAX_FILE_BYTES);
    assert!(parse(&exact[..]).is_ok());
    exact.push(b'\n');
    assert_eq!(parse(&exact[..]), Err(ObjImportError::TooLarge));
}

/// A reader of an endless repeated chunk that counts the bytes the parser consumed.
struct Endless {
    chunk: Vec<u8>,
    pulled: u64,
}

impl io::Read for Endless {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.chunk.len());
        out[..n].copy_from_slice(&self.chunk[..n]);
        self.pulled += n as u64;
        Ok(n)
    }
}

impl BufRead for Endless {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Ok(&self.chunk)
    }
    fn consume(&mut self, amount: usize) {
        self.pulled += amount as u64;
    }
}

#[test]
fn endless_input_is_bounded_by_the_caps() {
    let mut no_newline = Endless {
        chunk: vec![b'a'; 8192],
        pulled: 0,
    };
    assert_eq!(
        parse(&mut no_newline),
        Err(ObjImportError::LineTooLong { line: 1 })
    );
    assert!(no_newline.pulled <= MAX_LINE_BYTES as u64 + 2);
    let mut tokens = Endless {
        chunk: b"f 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 ".repeat(256),
        pulled: 0,
    };
    assert_eq!(
        parse(&mut tokens),
        Err(ObjImportError::LineTooLong { line: 1 })
    );
    assert!(tokens.pulled <= MAX_LINE_BYTES as u64 + 2);
    let mut comments = Endless {
        chunk: b"#\n".repeat(4096),
        pulled: 0,
    };
    assert_eq!(parse(&mut comments), Err(ObjImportError::TooLarge));
    assert!(comments.pulled <= MAX_FILE_BYTES + 1);
}

#[test]
fn read_names_unnamed_objects_after_the_file_stem_and_refuses_non_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("my model.obj");
    std::fs::write(&path, with_tri("o\nf 3 2 1\n")).unwrap();
    let parsed = read(&path).unwrap();
    let names: Vec<_> = parsed.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["my model", "my model"]);
    assert_eq!(read(directory.path()), Err(ObjImportError::NotRegularFile));
    assert_eq!(
        read(&directory.path().join("none.obj")),
        Err(ObjImportError::Io(io::ErrorKind::NotFound))
    );
}

#[test]
fn a_carriage_return_at_the_chunk_boundary_is_not_a_line_end() {
    let comment = |len: usize| format!("#{}", "x".repeat(len - 1));
    // CRLF terminates a line of exactly MAX_LINE_BYTES of content.
    assert!(parse_text(&format!("{TRI}{}\r\n", comment(MAX_LINE_BYTES))).is_ok());
    // A bare CR is never a line end: it is refused, not split into two lines.
    assert_eq!(
        err(&format!("{TRI}{}\rf 1 2 3\n", comment(MAX_LINE_BYTES))),
        ObjImportError::CrOnlyLineEndings { line: 5 }
    );
    // A CR that is only the last byte of an over-long chunk may start a CRLF: still too long.
    assert_eq!(
        err(&format!("{TRI}{}\r\n", comment(MAX_LINE_BYTES + 1))),
        ObjImportError::LineTooLong { line: 5 }
    );
    assert_eq!(
        err(&format!("{TRI}{}\r\n", comment(MAX_LINE_BYTES + 1))),
        ObjImportError::LineTooLong { line: 5 }
    );
}

#[test]
fn extra_vertex_tokens_must_be_numbers() {
    let vertex = |extra: &str| parse_text(&format!("v 0 0 0 {extra}\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"));
    assert!(vertex("1").is_ok());
    assert!(vertex("0.5 0.5 0.5").is_ok());
    for junk in ["junk", "1 x", "0x1"] {
        assert_eq!(
            vertex(junk),
            Err(ObjImportError::Syntax {
                line: 1,
                what: SyntaxKind::BadNumber
            }),
            "{junk}"
        );
    }
    assert_eq!(
        vertex("nan"),
        Err(ObjImportError::CoordinateOutOfRange { line: 1 })
    );
}

#[test]
fn cr_only_files_are_refused_with_a_clear_error() {
    let small = "v 0 0 0\rv 1 0 0\rv 0 1 0\rf 1 2 3\r";
    assert_eq!(err(small), ObjImportError::CrOnlyLineEndings { line: 1 });
    assert!(
        err(small)
            .to_string()
            .contains("CR-only line endings are not supported")
    );
    let large = small.repeat(20_000);
    assert!(large.len() > MAX_LINE_BYTES * 4);
    assert_eq!(err(&large), ObjImportError::CrOnlyLineEndings { line: 1 });
    assert_eq!(
        err(&format!("{TRI}# c\rv 0 0 0\n")),
        ObjImportError::CrOnlyLineEndings { line: 5 }
    );
    // A lone CR as the very last byte is trailing whitespace, not a line ending.
    assert!(parse_text(&format!("{TRI}\r")).is_ok());
}
