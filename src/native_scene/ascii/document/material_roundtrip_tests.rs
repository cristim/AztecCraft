//! Writer and gate agree on session materials (#156): generated scenes (random shape counts,
//! 0 to 8 materials incl. unassigned ones and gapped `lambertN`, random assignments) are
//! emitted, read back as authored with an equal scene and re-saved byte-identically.
use super::material_tests::{block, file};
use super::tests::{authored, refused, replace};
use super::*;
use crate::{
    native_scene::{MaterialChoice, ascii::write::emit_geometry_prefix},
    scene::Primitive,
};

fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

fn generated(state: &mut u64, round: usize) -> NativeScene {
    let primitives = [Primitive::Cube, Primitive::Plane, Primitive::Sphere];
    let mut scene = NativeScene::default();
    for index in 0..next(state) % 6 {
        let primitive = primitives[(next(state) % 3) as usize];
        scene
            .create(primitive, &format!("m{round}_{index}"))
            .unwrap();
    }
    let ids: Vec<_> = scene.shapes().iter().map(|s| s.id()).collect();
    for _ in 0..next(state) % 9 {
        let color = [0, 1, 2].map(|_| (next(state) % 1000) as f64 / 999.0);
        // Not every new material is assigned: some stay unassigned.
        if ids.is_empty() || next(state).is_multiple_of(3) {
            scene.create_session_material(color).unwrap();
        } else {
            let shape = ids[(next(state) as usize) % ids.len()];
            scene
                .assign_session_material(shape, MaterialChoice::New { color })
                .unwrap();
        }
    }
    // Reassign among existing materials, and back to the default group.
    let count = scene.session_materials().len();
    for &shape in &ids {
        if next(state).is_multiple_of(3) {
            let pick = (next(state) as usize) % count;
            let id = crate::native_scene::SessionMaterialId::from_index(pick);
            scene
                .assign_session_material(shape, MaterialChoice::Existing(id))
                .unwrap();
        }
    }
    // Gaps in the lambert numbering: drop the middle of the table by renumbering one pair.
    if scene.session_materials().len() > 2 && next(state).is_multiple_of(2) {
        let table = &mut scene.session_materials;
        let last = table.materials.len() - 1;
        table.materials[last].name = format!("lambert{}", 40 + round);
        table.groups[last].name = format!("lambert{}SG", 40 + round);
    }
    scene
}

#[test]
fn generated_scenes_round_trip_authored_equal_and_byte_identical() {
    let mut state = 156_303u64;
    let (mut with_materials, mut unassigned, mut gapped) = (0, 0, 0);
    for round in 0..120 {
        let scene = generated(&mut state, round);
        let bytes = emit_geometry_prefix(&scene).unwrap();
        let projection = authored(&bytes);
        assert_eq!(projection.scene().shapes(), scene.shapes(), "round {round}");
        assert_eq!(projection.scene().nodes(), scene.nodes(), "round {round}");
        assert_eq!(
            projection.scene().session_materials,
            scene.session_materials,
            "round {round}"
        );
        // Save, reopen and save again.
        assert_eq!(
            emit_geometry_prefix(projection.scene()).unwrap(),
            bytes,
            "round {round}"
        );
        // The independent spelling of the block is the emitted tail.
        if scene.session_materials().len() > 1 {
            with_materials += 1;
            assert!(
                String::from_utf8(bytes).unwrap().contains(&block(&scene)),
                "round {round}"
            );
            let table = &scene.session_materials;
            unassigned += usize::from(
                (1..table.groups.len()).any(|g| !table.assignments.values().any(|a| *a == g)),
            );
            gapped += usize::from(
                table
                    .materials
                    .iter()
                    .any(|m| m.name().starts_with("lambert4")),
            );
        }
    }
    assert!(with_materials > 40 && unassigned > 10 && gapped > 5);
}

#[test]
fn a_saved_material_scene_with_the_default_link_added_or_swapped_is_refused() {
    let model = super::material_tests::scene();
    let bytes = file(&model);
    // Both the default and the assigned link for one shape.
    let both = replace(
        &bytes,
        "connectAttr \"aShape.iog\" \"lambert2SG.dsm\" -na;\n",
        "connectAttr \"aShape.iog\" \":initialShadingGroup.dsm\" -na;\nconnectAttr \"aShape.iog\" \"lambert2SG.dsm\" -na;\n",
    );
    assert!(matches!(refused(&both), AsciiDocumentError::Ascii(_)));
    // Links in another shape order than the writer's.
    let a = "connectAttr \"aShape.iog\" \"lambert2SG.dsm\" -na;\n";
    let c = "connectAttr \"cShape.iog\" \"lambert3SG.dsm\" -na;\n";
    let misordered = replace(&replace(&bytes, a, "@@"), c, a);
    let misordered = replace(&misordered, "@@", c);
    assert!(matches!(refused(&misordered), AsciiDocumentError::Ascii(_)));
    // Material statements without links: no layout the writer emits.
    let text = String::from_utf8(bytes).unwrap();
    let unlinked: String = text
        .lines()
        .filter(|l| !l.contains(".iog"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(matches!(
        refused(unlinked.as_bytes()),
        AsciiDocumentError::Ascii(_)
    ));
}

#[test]
fn files_without_materials_keep_both_legacy_layouts() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "a").unwrap();
    for links in [DefaultLinks::Omit, DefaultLinks::Emit] {
        let bytes = emit_with_links(&scene, links).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("createNode lambert"));
        assert_eq!(
            emit_with_links(authored(&bytes).scene(), links).unwrap(),
            bytes
        );
    }
}
