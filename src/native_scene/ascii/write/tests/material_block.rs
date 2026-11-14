//! Byte accounting of the session material block (#156): the count and the emitter share
//! `session_block`, so they agree and the 16 MiB cap and the resource policy apply to it.

use super::*;
use crate::native_scene::MaterialChoice;

fn with_materials(count: usize) -> NativeScene {
    let mut scene = scene(&["a", "bb", "ccc"]);
    let ids: Vec<_> = scene.shapes().iter().map(|s| s.id()).collect();
    for index in 0..count {
        let color = [index as f64 / 8.0, 0.5, 1.0];
        let shape = ids.get(index % 2).copied().filter(|_| index < 2);
        match shape {
            Some(shape) => {
                scene
                    .assign_session_material(shape, MaterialChoice::New { color })
                    .unwrap();
            }
            None => {
                scene.create_session_material(color).unwrap();
            }
        }
    }
    scene
}

fn block_bytes(scene: &NativeScene) -> usize {
    let mut total = 0;
    session_block(scene, DefaultLinks::Emit, |bytes| {
        total += bytes.len();
        Ok(())
    })
    .unwrap();
    total
}

#[test]
fn the_material_block_is_counted_exactly_and_follows_every_node() {
    let model = with_materials(4);
    let plain = scene(&["a", "bb", "ccc"]);
    let counted = prefix_bytes(&model, DefaultLinks::Emit).unwrap();
    let bytes = emit_with_links(&model, DefaultLinks::Emit).unwrap();
    assert_eq!(bytes.len(), counted);
    assert_eq!(
        counted - prefix_bytes(&plain, DefaultLinks::Emit).unwrap(),
        block_bytes(&model) - block_bytes(&plain)
    );
    // Materials force the links whatever the layout says, so both layouts agree.
    assert_eq!(emit_with_links(&model, DefaultLinks::Omit).unwrap(), bytes);
    let text = String::from_utf8(bytes).unwrap();
    let first_block = text.find("createNode lambert").unwrap();
    assert!(text.rfind("createNode mesh").unwrap() < first_block);
    assert!(!text[first_block..].contains("createNode transform"));
    assert!(text.ends_with("\"cccShape.iog\" \":initialShadingGroup.dsm\" -na;\n"));
}

#[test]
fn one_byte_over_the_cap_refuses_and_the_closure_propagates_each_consumer_error() {
    let model = with_materials(3);
    let block = block_bytes(&model);
    assert_eq!(add_prefix_bytes(MAX_BYTES - block, block), Ok(MAX_BYTES));
    assert_eq!(
        add_prefix_bytes(MAX_BYTES - block + 1, block),
        Err(AsciiWriteRefusal::TooLarge)
    );
    let mut calls = 0;
    session_block(&model, DefaultLinks::Emit, |_| {
        calls += 1;
        Ok(())
    })
    .unwrap();
    for fail_at in 0..calls {
        let mut seen = 0;
        let result = session_block(&model, DefaultLinks::Emit, |_| {
            seen += 1;
            if seen == fail_at + 1 {
                Err(AsciiWriteRefusal::TooLarge)
            } else {
                Ok(())
            }
        });
        assert_eq!(result, Err(AsciiWriteRefusal::TooLarge), "call {fail_at}");
    }
    let mut out = Vec::new();
    assert_eq!(
        session_block(&model, DefaultLinks::Emit, |bytes| append(
            &mut out,
            block - 1,
            bytes
        )),
        Err(AsciiWriteRefusal::TooLarge)
    );
}

fn admitted_pad(base: &NativeScene) -> usize {
    let padded = |length: usize| {
        let mut padded = base.clone();
        padded.nodes[0].name = "p".repeat(length);
        resource_admission(&padded).is_ok()
    };
    let (mut admitted, mut refused) = (1usize, 1 << 20);
    assert!(padded(admitted) && !padded(refused));
    while refused - admitted > 1 {
        let middle = admitted + (refused - admitted) / 2;
        if padded(middle) {
            admitted = middle;
        } else {
            refused = middle;
        }
    }
    admitted
}

#[test]
fn the_material_block_is_charged_to_resource_admission_and_bounded() {
    let mut plain = scene(&["a"]);
    for index in 1..100 {
        plain.create(Primitive::Cube, &format!("n{index}")).unwrap();
    }
    let mut model = plain.clone();
    let shape = model.shapes()[0].id();
    model
        .assign_session_material(
            shape,
            MaterialChoice::New {
                color: [0.1, 0.2, 0.3],
            },
        )
        .unwrap();
    for _ in 1..8 {
        model.create_session_material([0.5, 0.5, 0.5]).unwrap();
    }
    let table = &model.session_materials;
    let names: usize = table.materials[1..]
        .iter()
        .map(|m| m.name().len())
        .chain(table.groups[1..].iter().map(|g| g.name().len()))
        .sum();
    // Plus the existing 32-per-entry count charge: 7 more materials, 7 more groups, 1 assignment.
    let charge = 8 * MATERIAL_SYNTAX_BYTES + 3 * names + "lambert2SG".len() + 32 * 15;
    let rate = model.nodes.len() + model.shapes.len() + 2;
    let drop = admitted_pad(&plain) - admitted_pad(&model);
    assert!(
        (charge / rate..=charge.div_ceil(rate)).contains(&drop),
        "dropped {drop}"
    );
    // Past the cap the table is refused before any traversal of it.
    let mut oversized = scene(&["a"]);
    let (material, group) = (
        oversized.session_materials.materials[0].clone(),
        oversized.session_materials.groups[0].clone(),
    );
    for _ in 0..MAX_SESSION_MATERIALS {
        oversized.session_materials.materials.push(material.clone());
        oversized.session_materials.groups.push(group.clone());
    }
    assert!(resource_admission(&oversized).is_err());
    assert_eq!(prefix(&oversized), Err(AsciiWriteRefusal::ResourceLimit));
}
