//! An editable binary scene with a child "c" under a parent "p" (under an optional grandparent
//! "g"), for the Scale Parent tests: an ASCII scene is read-only, and creation makes only roots.
use super::*;

#[allow(dead_code)]
mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

/// `grandparent_scale` adds "g" with that scale above "p"; "p" turns `parent_z` radians about Z and
/// "c" turns `child_z`. "c" is selected.
pub(super) fn parent_child_editor(
    grandparent_scale: Option<[f64; 3]>,
    parent_z: f64,
    child_z: f64,
) -> Editor {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene, triple};
    let xform = |name: &[u8], parent: Option<&[u8]>, scale: [f64; 3], z: f64| {
        let mut leaves = creation(name, parent);
        leaves.extend(triple(b"r", [0.0, 0.0, z]));
        leaves.extend(triple(b"s", scale));
        group(b"XFRM", &leaves, 4)
    };
    let mut bytes = Vec::new();
    let mut parent_of_p = None;
    if let Some(scale) = grandparent_scale {
        bytes.extend(xform(b"g", None, scale, 0.0));
        parent_of_p = Some(&b"g"[..]);
    }
    bytes.extend(xform(b"p", parent_of_p, [1.0; 3], parent_z));
    bytes.extend(xform(b"c", Some(b"p"), [1.0; 3], child_z));
    bytes.extend(group(
        b"DMSH",
        &[
            creation(b"cShape", Some(b"c")),
            chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
        ]
        .concat(),
        4,
    ));
    let scene = azteccraft::native_scene::binary::decode(scene(&bytes)).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    select_child(&mut editor);
    editor
}

/// Selects the node named "c" alone.
pub(super) fn select_child(editor: &mut Editor) {
    let child = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| {
            editor
                .history
                .scene()
                .node(target.node)
                .is_ok_and(|node| node.name() == "c")
        })
        .unwrap();
    editor.set_selection_members_for_test(vec![child]);
    editor.sync_fields();
}
