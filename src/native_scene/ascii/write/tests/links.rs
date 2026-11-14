//! Byte accounting of the default shading-group link block (#156): the count and the emitter
//! share one closure, so they agree and the 16 MiB cap applies to the block.

use super::*;

#[test]
fn the_link_block_is_counted_exactly_and_emitted_after_every_node() {
    let scene = scene(&["a", "bb"]);
    let omit = prefix_bytes(&scene, DefaultLinks::Omit).unwrap();
    let emit = prefix_bytes(&scene, DefaultLinks::Emit).unwrap();
    let names: usize = scene.shapes.iter().map(|s| s.name().len()).sum();
    assert_eq!(emit - omit, 2 * LINK_SYNTAX_BYTES + names);
    let bytes = emit_with_links(&scene, DefaultLinks::Emit).unwrap();
    assert_eq!(bytes.len(), emit);
    let tail = "connectAttr \"aShape.iog\" \":initialShadingGroup.dsm\" -na;\nconnectAttr \"bbShape.iog\" \":initialShadingGroup.dsm\" -na;\n";
    assert!(bytes.ends_with(tail.as_bytes()));
    assert_eq!(
        emit_with_links(&scene, DefaultLinks::Omit).unwrap().len(),
        omit
    );
}

#[test]
fn one_byte_over_the_cap_refuses_and_the_closure_propagates_each_consumer_error() {
    assert_eq!(
        add_prefix_bytes(MAX_BYTES - LINK_SYNTAX_BYTES, LINK_SYNTAX_BYTES),
        Ok(MAX_BYTES)
    );
    assert_eq!(
        add_prefix_bytes(MAX_BYTES - LINK_SYNTAX_BYTES + 1, LINK_SYNTAX_BYTES),
        Err(AsciiWriteRefusal::TooLarge)
    );
    let scene = scene(&["a", "b"]);
    for fail_at in 0..6 {
        let mut calls = 0;
        let result = default_links(&scene, |_| {
            calls += 1;
            if calls == fail_at + 1 {
                Err(AsciiWriteRefusal::TooLarge)
            } else {
                Ok(())
            }
        });
        assert_eq!(result, Err(AsciiWriteRefusal::TooLarge), "call {fail_at}");
    }
    // The emitter's own append refuses a block that does not fit the admitted length.
    let mut out = Vec::new();
    assert_eq!(
        default_links(&scene, |bytes| append(&mut out, 10, bytes)),
        Err(AsciiWriteRefusal::TooLarge)
    );
}

#[test]
fn the_link_block_is_charged_to_resource_admission() {
    // One name's length moves the admitted work by (nodes + shapes + 2) = 202 per byte, so the
    // largest admitted pad pins the budget left over by everything else, the link block included.
    let mut scene = scene(&["a"]);
    for index in 1..100 {
        scene.create(Primitive::Cube, &format!("n{index}")).unwrap();
    }
    let padded = |length: usize| {
        let mut padded = scene.clone();
        padded.nodes[0].name = "p".repeat(length);
        resource_admission(&padded).is_ok()
    };
    assert!(padded(1));
    let (mut admitted, mut refused) = (1usize, 1 << 20);
    assert!(!padded(refused));
    while refused - admitted > 1 {
        let middle = admitted + (refused - admitted) / 2;
        if padded(middle) {
            admitted = middle;
        } else {
            refused = middle;
        }
    }
    assert!(padded(admitted) && !padded(admitted + 1));
    // The largest pad admitted before the link block was charged was UNLINKED_PAD. The block
    // costs shapes * LINK_SYNTAX_BYTES work, i.e. that many bytes of pad at `rate` work each
    // (the final partial byte depends on the slack, hence the two bounds).
    const UNLINKED_PAD: usize = 238_238;
    let rate = scene.nodes.len() + scene.shapes.len() + 2;
    let charge = scene.shapes.len() * LINK_SYNTAX_BYTES;
    assert!(
        (charge / rate..=charge.div_ceil(rate)).contains(&(UNLINKED_PAD - admitted)),
        "admitted {admitted}"
    );
}
