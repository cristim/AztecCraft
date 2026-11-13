//! Oracle: the expected tables are written by hand from the flags the Maya 2027 `file` command
//! page defines (-r, -ns, -rfn, -rpr, -typ, -dr, -rdi), not derived from the parser.
use super::*;
use crate::native_scene::{ReferenceDeclaration, ReferencePathKind};

fn input(body: &str) -> Arc<[u8]> {
    Arc::from(format!("//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; {body}").into_bytes())
}

fn declared(body: &str) -> Vec<ReferenceDeclaration> {
    decode(input(body)).unwrap().references().to_vec()
}

fn refused(body: &str) -> ErrorKind {
    decode(input(body)).unwrap_err().kind()
}

fn bare(kind: ReferencePathKind, path: Option<&str>) -> ReferenceDeclaration {
    ReferenceDeclaration {
        path_kind: kind,
        path: path.map(str::to_owned),
        namespace: None,
        reference_node: None,
        renaming_prefix: None,
        file_type: None,
        deferred: None,
        depth_info: None,
    }
}

#[test]
fn full_declaration_is_parsed_as_authored() {
    let got = declared(
        "file -r -ns \"cube\" -dr 1 -rfn \"cubeRN\" -rpr \"old\" -rdi 2 -typ \"mayaAscii\" \"assets/cube.ma\";",
    );
    let want = ReferenceDeclaration {
        namespace: Some("cube".into()),
        reference_node: Some("cubeRN".into()),
        renaming_prefix: Some("old".into()),
        file_type: Some("mayaAscii".into()),
        deferred: Some(true),
        depth_info: Some(2),
        ..bare(ReferencePathKind::Relative, Some("assets/cube.ma"))
    };
    assert_eq!(got, [want]);
}

#[test]
fn long_flag_names_and_deferred_false() {
    let got = declared("file -reference -namespace \"a\" -deferReference 0 \"a.ma\";");
    let want = ReferenceDeclaration {
        namespace: Some("a".into()),
        deferred: Some(false),
        ..bare(ReferencePathKind::Relative, Some("a.ma"))
    };
    assert_eq!(got, [want]);
}

#[test]
fn declaration_order_is_preserved() {
    let got = declared("file -r \"a.ma\"; file -r \"b.ma\";");
    let paths: Vec<_> = got.iter().map(|r| r.path.as_deref()).collect();
    assert_eq!(paths, [Some("a.ma"), Some("b.ma")]);
}

#[test]
fn private_paths_are_not_echoed() {
    for (path, kind) in [
        ("/Users/someone/shot.ma", ReferencePathKind::Absolute),
        ("C:/Users/someone/shot.ma", ReferencePathKind::Absolute),
        ("~/shot.ma", ReferencePathKind::Absolute),
        ("//server/share/shot.ma", ReferencePathKind::Absolute),
        ("$HOME/shot.ma", ReferencePathKind::Absolute),
        ("file:///Users/someone/shot.ma", ReferencePathKind::Absolute),
        ("%USERPROFILE%/shot.ma", ReferencePathKind::Absolute),
        (".//Users/someone/shot.ma", ReferencePathKind::Absolute),
        ("${ASSETS}/shot.ma", ReferencePathKind::Absolute),
        ("../../secret/shot.ma", ReferencePathKind::Escaping),
        ("a/../../b.ma", ReferencePathKind::Escaping),
    ] {
        let got = declared(&format!("file -r \"{path}\";"));
        assert_eq!(got, [bare(kind, None)], "{path}");
    }
}

#[test]
fn unproven_or_malformed_declarations_refuse() {
    for body in [
        "file \"a.ma\";",
        "file -rdi 1 \"a.ma\";",
        "file -r -op \"v=0;\" \"a.ma\";",
        "file -r -lrd \"all\" \"a.ma\";",
        "file -r -lck 1 \"a.ma\";",
        "file -r -iv 1 \"a.ma\";",
        "file -r -dns \"x\" \"a.ma\";",
        "file -r -r \"a.ma\";",
        "file -r -ns \"a\" -ns \"b\" \"a.ma\";",
        "file -r -dr 2 \"a.ma\";",
        "file -r -rdi -1 \"a.ma\";",
        "file -r -ns \"a\\\\b\" \"a.ma\";",
        "file -r \"a\\\\b.ma\";",
        "file -r \"\";",
        "file -r \"a.ma\" \"b.ma\";",
        "file -r \" /Users/someone/x.ma\";",
        "file -r \"a.ma \";",
        "file -r \"a\u{202e}b.ma\";",
        "file -r -ns \"a\u{200b}\" \"a.ma\";",
        "file -r;",
        "file -r -ns;",
    ] {
        assert!(decode(input(body)).is_err(), "{body}");
    }
    assert_eq!(
        refused("file -r -op \"v=0;\" \"a.ma\";"),
        ErrorKind::Unsupported
    );
}

#[test]
fn counts_and_sizes_are_bounded() {
    let many = "file -r \"a.ma\"; ".repeat(33);
    assert_eq!(refused(&many), ErrorKind::ResourceLimit);
    assert_eq!(declared(&"file -r \"a.ma\"; ".repeat(32)).len(), 32);
    let long_path = "x".repeat(257);
    assert_eq!(
        refused(&format!("file -r \"{long_path}\";")),
        ErrorKind::ResourceLimit
    );
    let long_ns = "n".repeat(129);
    assert_eq!(
        refused(&format!("file -r -ns \"{long_ns}\" \"a.ma\";")),
        ErrorKind::Unsupported
    );
}

#[test]
fn referenced_nodes_still_refuse() {
    // A namespaced node is what a loaded reference would contribute; it cannot be preserved.
    assert!(
        decode(input(
            "file -r -ns \"c\" \"a.ma\"; createNode transform -n \"c:cube\";"
        ))
        .is_err()
    );
    assert!(
        decode(input(
            "file -r -rfn \"cRN\" \"a.ma\"; createNode reference -n \"cRN\";"
        ))
        .is_err()
    );
}

#[test]
fn reference_scene_is_read_only() {
    let scene = decode(input("file -r \"a.ma\";")).unwrap();
    assert!(scene.authoring_read_only_reason().is_some());
}
