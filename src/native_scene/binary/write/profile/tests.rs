use super::*;
use crate::native_scene::binary::write::fixtures::{MAYA_2025, MAYA_2026, groups, leaves, walk};

// Integrity of the vendored files is the recorded length plus the offset and
// byte-identity pins below; no hash is computed in the repo.
#[test]
fn the_vendored_fixtures_have_the_recorded_sizes() {
    assert_eq!(MAYA_2025.len(), 553_616);
    assert_eq!(MAYA_2026.len(), 8096);
}

#[test]
fn the_walker_consumes_each_fixture_to_its_last_byte() {
    for file in [MAYA_2025, MAYA_2026] {
        let top: Vec<_> = walk(file)
            .into_iter()
            .filter(|e| e.parent_form.is_none())
            .collect();
        assert_eq!(top.last().map(|e| e.end), Some(file.len()));
        assert!(top.windows(2).all(|pair| pair[0].end == pair[1].start));
    }
}

#[test]
fn the_fixture_is_a_maya_2025_file() {
    let finf: Vec<_> = MAYA_2025
        .windows(b"Maya 2025".len())
        .filter(|w| *w == b"Maya 2025")
        .collect();
    assert_eq!(finf.len(), 1);
    assert!(
        MAYA_2025
            .windows(b"202409190603-cbdc5a7e54".len())
            .any(|w| w == b"202409190603-cbdc5a7e54")
    );
}

#[test]
fn vers_is_the_string_the_fixture_stores_at_its_first_head_child() {
    let entries = walk(MAYA_2025);
    let vers = leaves(&entries, b"VERS", b"HEAD");
    assert_eq!(vers.len(), 1);
    assert_eq!(vers[0].start, 0x28);
    assert_eq!(vers[0].aux, VERS_AUX);
    assert_eq!(vers[0].bytes(MAYA_2025), VERS);
}

#[test]
fn root_and_group_aux_are_what_the_fixture_stores_on_those_groups() {
    let entries = walk(MAYA_2025);
    let first = |form: &[u8; 4]| groups(&entries, form)[0].clone();
    assert_eq!(first(b"Maya").start, 0);
    assert_eq!(first(b"Maya").aux, ROOT_AUX);
    assert_eq!(first(b"HEAD").start, 0x14);
    assert_eq!(first(b"HEAD").aux, HEAD_GROUP_AUX);
    let xfrm = groups(&entries, b"XFRM");
    assert_eq!(xfrm.len(), 5);
    assert!(xfrm.iter().all(|g| g.aux == XFRM_GROUP_AUX));
}

#[test]
fn every_head_child_aux_is_the_value_the_fixture_stores_for_that_kind() {
    let entries = walk(MAYA_2025);
    let cases: [(&[u8; 4], u32, usize); 11] = [
        (b"VERS", VERS_AUX, 0x28),
        (b"UVER", UVER_AUX, 0x40),
        (b"MADE", MADE_AUX, 0x58),
        (b"ICON", ICON_AUX, 0xa0),
        (b"INFO", INFO_AUX, 0xb8),
        (b"OBJN", OBJN_AUX, 0xd0),
        (b"INCL", INCL_AUX, 0xe8),
        (b"LUNI", LUNI_AUX, 0x100),
        (b"TUNI", TUNI_AUX, 0x118),
        (b"AUNI", AUNI_AUX, 0x130),
        (b"TDUR", TDUR_AUX, 0x148),
    ];
    for (tag, aux, offset) in cases {
        let first = &leaves(&entries, tag, b"HEAD")[0];
        assert_eq!(first.start, offset, "{tag:?}");
        assert_eq!(first.aux, aux, "{tag:?}");
    }
}

#[test]
fn xfrm_children_use_the_fixture_aux_for_crea_and_dbl3() {
    let entries = walk(MAYA_2025);
    let crea = leaves(&entries, b"CREA", b"XFRM");
    assert_eq!(crea.len(), 5);
    assert_eq!(crea[0].start, 0x394);
    assert!(crea.iter().all(|c| c.aux == XFRM_CREA_AUX));
    let dbl3 = leaves(&entries, b"DBL3", b"XFRM");
    assert_eq!(dbl3.len(), 7);
    assert_eq!(dbl3[0].start, 0x3dc);
    assert!(dbl3.iter().all(|c| c.aux == DBL3_AUX));
}

#[test]
fn the_user_node_is_0x8c_and_the_four_default_cameras_are_0x8d() {
    let entries = walk(MAYA_2025);
    let flags: Vec<_> = leaves(&entries, b"CREA", b"XFRM")
        .iter()
        .map(|c| {
            let bytes = c.bytes(MAYA_2025);
            let name = bytes[1..].split(|b| *b == 0).next().unwrap().to_vec();
            (c.start, name, bytes[0])
        })
        .collect();
    let camera = |at, name: &[u8]| (at, name.to_vec(), 0x8d);
    assert_eq!(
        flags[..4],
        [
            camera(0x394, b"persp"),
            camera(0x5bc, b"top"),
            camera(0x814, b"front"),
            camera(0xa4c, b"side"),
        ]
    );
    assert_eq!(flags[4], (0xca4, b"Dummy_Mesh".to_vec(), XFRM_CREA_FLAGS));
    assert_eq!(flags.len(), 5);
    assert_eq!(XFRM_CREA_FLAGS, 0x8c);
}

#[test]
fn the_2026_fixture_stores_other_values_so_mixing_releases_would_show() {
    let entries = walk(MAYA_2026);
    assert_ne!(leaves(&entries, b"DBL3", b"XFRM")[0].aux, DBL3_AUX);
    assert_ne!(groups(&entries, b"Maya")[0].aux, ROOT_AUX);
}
