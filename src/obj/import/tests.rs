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
