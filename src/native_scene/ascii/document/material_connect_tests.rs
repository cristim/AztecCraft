//! Session-material connects (#156): the reader reads exactly the connects the writer will emit
//! and refuses every other form. See `material_tests` for the file layout.
use super::material_tests::{file, read, read_err, scene};
use super::tests::replace;
use super::*;

#[test]
fn connects_the_writer_would_not_emit_are_refused() {
    let bytes = file(&scene());
    let oc = "connectAttr \"lambert2.oc\" \"lambert2SG.ss\";\n";
    let pa = "connectAttr \"lambert2SG.pa\" \":renderPartition.st\" -na;\n";
    let msg = "connectAttr \"lambert2.msg\" \":defaultShaderList1.s\" -na;\n";
    let link = "connectAttr \"aShape.iog\" \"lambert2SG.dsm\" -na;\n";
    let both = "connectAttr \"aShape.iog\" \":initialShadingGroup.dsm\" -na;\n";
    let cases = [
        ("duplicate oc", replace(&bytes, oc, &format!("{oc}{oc}"))),
        ("duplicate pa", replace(&bytes, pa, &format!("{pa}{pa}"))),
        (
            "duplicate msg",
            replace(&bytes, msg, &format!("{msg}{msg}")),
        ),
        (
            "duplicate membership",
            replace(&bytes, link, &format!("{link}{link}")),
        ),
        (
            "default and assigned",
            replace(&bytes, link, &format!("{link}{both}")),
        ),
        (
            "oc to another group",
            replace(
                &bytes,
                oc,
                "connectAttr \"lambert2.oc\" \"lambert3SG.ss\";\n",
            ),
        ),
        (
            "swapped pairing",
            replace(
                &bytes,
                oc,
                "connectAttr \"lambert3.oc\" \"lambert2SG.ss\";\n",
            ),
        ),
        (
            "pa without -na",
            replace(
                &bytes,
                pa,
                "connectAttr \"lambert2SG.pa\" \":renderPartition.st\";\n",
            ),
        ),
        (
            "membership without -na",
            replace(
                &bytes,
                link,
                "connectAttr \"aShape.iog\" \"lambert2SG.dsm\";\n",
            ),
        ),
    ];
    for (name, edited) in cases {
        assert!(read(&edited).is_err(), "{name}");
    }
    // A connect before either node exists reads no node.
    let early = replace(
        &bytes,
        "createNode transform",
        &format!("{oc}createNode transform"),
    );
    assert_eq!(read_err(&early), ErrorKind::Unsupported);
}
