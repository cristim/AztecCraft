// A complete zero-padded scene whose transforms lack a translate leaf but record where one may go (issue #19).
// Include inside the same module as `maya_binary_sites.rs`, whose builders it uses.

/// Two transforms: `object` (rotate leaf only, so translate is insertable and scale is not) and
/// `other` (scale leaf only, so rotate is not), then a mesh shape under `object`. `mesh` is a MESH payload.
pub(crate) fn sited_scene(mesh: &[u8]) -> Vec<u8> {
    let header = [
        chunk_zero_pad(b"LUNI", 0, b"cm", 8),
        chunk_zero_pad(b"AUNI", 0, b"deg", 8),
        chunk_zero_pad(b"TUNI", 0, b"film", 8),
    ]
    .concat();
    let children = [
        group_zero_pad(b"HEAD", &header, 4),
        sited_transform(b"object", &[triple_aux(b"r", SITE_AUX, [0.0, 0.0, 0.25])]),
        sited_transform(b"other", &[triple_aux(b"s", SITE_AUX, [2.0, 2.0, 2.0])]),
        group_zero_pad(
            b"DMSH",
            &[
                creation_zero_pad(b"shape", Some(b"object")),
                chunk_zero_pad(b"MESH", 0, mesh, 8),
            ]
            .concat(),
            4,
        ),
    ]
    .concat();
    group_zero_pad(b"Maya", &children, 1)
}
