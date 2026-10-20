use super::{
    site_fixture::{
        SITE_AUX, chunk_zero_pad, creation_zero_pad, double_aux, group_zero_pad,
        other_node_with_triple, sited_transform, triple_aux,
    },
    wire_fixture::scene,
};
use crate::native_scene::binary::decode;

#[test]
fn zero_pad_builders_emit_exact_bytes() {
    let record = triple_aux(b"r", SITE_AUX, [1.0, 2.0, 3.0]);
    assert_eq!(record.len(), 48);
    assert_eq!(&record[..4], b"DBL3");
    assert_eq!(record[4..8], SITE_AUX.to_be_bytes());
    assert_eq!(record[8..16], 27u64.to_be_bytes());
    assert_eq!(&record[16..19], b"r\0\x20");
    assert_eq!(record[19..27], 1.0f64.to_be_bytes());
    assert_eq!(record[43..], [0; 5]);
    let visible = double_aux(b"v", 0x24, SITE_AUX, 1.0);
    assert_eq!(visible.len(), 32);
    assert_eq!(visible[16..19], *b"v\0\x24");
    assert_eq!(visible[27..], [0; 5]);
    let group = group_zero_pad(b"XFRM", &creation_zero_pad(b"a", None), 4);
    assert_eq!(&group[..4], b"FOR8");
    assert_eq!(group[4..8], [0; 4]);
    assert_eq!(chunk_zero_pad(b"ATTR", 7, b"abc", 8)[19..], [0; 5]);
}

#[test]
fn zero_pad_scene_decodes_with_translation_and_unrelated_node_triple() {
    let children = [
        sited_transform(
            b"object",
            &[
                double_aux(b"v", 0x24, SITE_AUX, 1.0),
                triple_aux(b"t", SITE_AUX, [4.0, 5.0, 6.0]),
            ],
        ),
        other_node_with_triple(b"light", SITE_AUX ^ 1),
    ]
    .concat();
    let decoded = decode(scene(&children)).unwrap();
    assert_eq!(decoded.nodes()[0].transform().translation, [4.0, 5.0, 6.0]);
    assert_eq!(decoded.nodes().len(), 1);
}

mod grammar {
    use super::super::{
        site_fixture::{
            SITE_AUX, chunk_zero_pad, double_aux, other_node_with_triple, sited_transform,
            triple_aux,
        },
        wire_fixture::{chunk, creation, group, mesh_payload, scene, triple},
    };
    use crate::{
        maya_binary::container,
        native_scene::binary::{ErrorKind, decode},
    };

    fn t() -> Vec<u8> {
        triple_aux(b"t", SITE_AUX, [1.0; 3])
    }
    fn r() -> Vec<u8> {
        triple_aux(b"r", SITE_AUX, [2.0; 3])
    }
    fn s() -> Vec<u8> {
        triple_aux(b"s", SITE_AUX, [3.0; 3])
    }
    fn rp() -> Vec<u8> {
        triple_aux(b"rp", SITE_AUX, [4.0; 3])
    }
    fn v() -> Vec<u8> {
        double_aux(b"v", 0x24, SITE_AUX, 1.0)
    }
    fn attribute() -> Vec<u8> {
        let mut bytes = b"aDBL\0\xf8\x00\x4e\0\0\0\x01\0\0\0\0my\0my\0\0\0\0\0".to_vec();
        bytes.extend(0.0f64.to_be_bytes());
        bytes.extend(1.0f64.to_be_bytes());
        bytes.extend(b"DBLE");
        bytes.extend(0.0f64.to_be_bytes());
        chunk(b"ATTR", &bytes, 8)
    }

    /// `Err` when the decoder itself refuses the file, else the sites as `(at, ancestors)`.
    fn sites(children: &[u8]) -> Result<Vec<(usize, Vec<usize>)>, ()> {
        let decoded = decode(scene(children)).map_err(|_| ())?;
        Ok(decoded
            .source
            .as_ref()
            .unwrap()
            .sites
            .iter()
            .map(|site| (site.at, site.ancestors.clone()))
            .collect())
    }

    /// The transform under test, then a node whose only job is to carry a canonical `DBL3`.
    fn file(parts: &[Vec<u8>]) -> Vec<u8> {
        [
            sited_transform(b"object", parts),
            other_node_with_triple(b"anchor", SITE_AUX),
        ]
        .concat()
    }

    fn one(parts: &[Vec<u8>]) -> Result<Vec<(usize, Vec<usize>)>, ()> {
        sites(&file(parts))
    }

    fn bytes_of(parts: &[Vec<u8>]) -> Vec<u8> {
        scene(&file(parts)).to_vec()
    }

    fn header_of(parts: &[Vec<u8>], tag: &[u8; 4], key: &[u8]) -> usize {
        let inspected = container::inspect(bytes_of(parts).into()).unwrap();
        let chunk = inspected
            .chunks()
            .iter()
            .find(|chunk| {
                chunk.tag() == *tag && inspected.source()[chunk.payload_range()].starts_with(key)
            })
            .unwrap();
        chunk.header_range().start
    }

    fn payload_end(parts: &[Vec<u8>]) -> usize {
        let inspected = container::inspect(bytes_of(parts).into()).unwrap();
        let group = inspected
            .chunks()
            .iter()
            .find(|chunk| chunk.form() == Some(*b"XFRM"))
            .unwrap();
        group.payload_range().end
    }

    fn group_start(parts: &[Vec<u8>]) -> usize {
        let inspected = container::inspect(bytes_of(parts).into()).unwrap();
        let group = inspected
            .chunks()
            .iter()
            .find(|chunk| chunk.form() == Some(*b"XFRM"))
            .unwrap();
        group.header_range().start
    }

    #[test]
    fn accepted_shapes_get_the_position_of_the_first_greater_record_or_the_group_end() {
        let at_end: [&[Vec<u8>]; 2] = [&[], &[v()]];
        for parts in at_end {
            let found = one(parts).unwrap();
            assert_eq!(
                found,
                [(payload_end(parts), vec![group_start(parts), 0])],
                "{parts:?}"
            );
        }
        let before_first: [(&[Vec<u8>], &[u8]); 5] = [
            (&[r()], b"r\0"),
            (&[s()], b"s\0"),
            (&[v(), r(), s()], b"r\0"),
            (&[v(), s(), rp()], b"s\0"),
            (&[r(), rp()], b"r\0"),
        ];
        for (parts, key) in before_first {
            let found = one(parts).unwrap();
            let at = header_of(parts, b"DBL3", key);
            assert_eq!(found, [(at, vec![group_start(parts), 0])]);
        }
    }

    #[test]
    fn existing_translation_leaf_has_no_site() {
        for parts in [vec![t()], vec![t(), r()], vec![v(), t()]] {
            assert_eq!(one(&parts), Ok(Vec::new()), "{parts:?}");
        }
    }

    #[test]
    fn any_record_outside_creation_visibility_and_ordered_triples_has_no_site() {
        for parts in [
            vec![attribute()],
            vec![r(), attribute()],
            vec![v(), attribute()],
        ] {
            assert_eq!(one(&parts), Ok(Vec::new()), "{parts:?}");
        }
    }

    #[test]
    fn visibility_after_a_triple_or_repeated_has_no_site() {
        for parts in [vec![r(), v()], vec![v(), v()], vec![v(), r(), v()]] {
            assert_eq!(one(&parts), Ok(Vec::new()), "{parts:?}");
        }
    }

    #[test]
    fn unordered_duplicate_or_pivot_first_keys_have_no_site() {
        for parts in [
            vec![s(), r()],
            vec![r(), r()],
            vec![rp(), r()],
            vec![rp()],
            vec![triple_aux(b"sp", SITE_AUX, [0.0; 3])],
            vec![r(), triple_aux(b"sp", SITE_AUX, [0.0; 3]), rp()],
        ] {
            assert_eq!(one(&parts), Ok(Vec::new()), "{parts:?}");
        }
    }

    #[test]
    fn keys_outside_the_decoder_table_and_foreign_children_never_decode() {
        for key in [b"sh".as_slice(), b"rpt", b"x"] {
            assert_eq!(one(&[triple_aux(key, SITE_AUX, [0.0; 3])]), Err(()));
        }
        assert_eq!(one(&[chunk(b"ZZZZ", b"k\0abc", 8)]), Err(()));
        let nested = group(b"SHAD", &[], 8);
        assert_eq!(one(std::slice::from_ref(&nested)), Err(()));
        let error = decode(scene(&file(&[nested]))).unwrap_err();
        assert_eq!(
            error.kind(),
            ErrorKind::Malformed,
            "inspect rejects it, not construct"
        );
    }

    #[test]
    fn shapes_get_no_site_and_each_transform_is_judged_alone() {
        let shape = group(
            b"DMSH",
            &[
                creation(b"shape", Some(b"object")),
                chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
            ]
            .concat(),
            4,
        );
        let children = [
            sited_transform(b"object", &[]),
            shape,
            sited_transform(b"other", &[t()]),
            sited_transform(b"third", &[r()]),
        ]
        .concat();
        let found = sites(&children).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].0 < found[1].0);
    }

    #[test]
    fn retention_validation_rejects_every_malformed_site() {
        let children = [sited_transform(b"a", &[r()]), sited_transform(b"b", &[])].concat();
        let source = decode(scene(&children)).unwrap().source.unwrap();
        assert_eq!(source.sites.len(), 2);
        assert_eq!(source.validate(), Ok(()));
        let inner_end = source.sites[1].at;
        type Mutation = fn(&mut crate::native_scene::source::SourceRetention);
        let mutations: [(&str, Mutation); 8] = [
            ("at inside a record", |s| s.sites[0].at += 1),
            ("at past the file", |s| s.sites[0].at = usize::MAX),
            ("at before the group body", |s| {
                s.sites[0].at = s.sites[0].ancestors[0] + 16
            }),
            ("no root ancestor", |s| {
                s.sites[0].ancestors.pop();
            }),
            ("no ancestors", |s| s.sites[0].ancestors.clear()),
            ("ancestors reversed", |s| s.sites[0].ancestors.reverse()),
            ("owner order", |s| s.sites.swap(0, 1)),
            ("field", |s| {
                s.sites[0].field = crate::native_scene::source::TransformField::Rotation
            }),
        ];
        for (name, mutate) in mutations {
            let mut broken = source.clone();
            mutate(&mut broken);
            assert_eq!(
                broken.validate(),
                Err(crate::native_scene::NativeError::InvalidRetention),
                "{name}"
            );
        }
        let mut sibling = source.clone();
        sibling.sites[0].at = inner_end;
        assert_eq!(
            sibling.validate(),
            Err(crate::native_scene::NativeError::InvalidRetention)
        );
    }

    fn no_site_with(parts: &[Vec<u8>], extra: &[u8]) -> Result<Vec<(usize, Vec<usize>)>, ()> {
        sites(&[sited_transform(b"object", parts), extra.to_vec()].concat())
    }

    #[test]
    fn a_file_without_any_triple_has_no_site() {
        for parts in [vec![], vec![v()]] {
            assert_eq!(no_site_with(&parts, &[]), Ok(Vec::new()), "{parts:?}");
        }
    }

    #[test]
    fn triples_with_differing_aux_anywhere_leave_the_file_siteless() {
        let other = |aux| other_node_with_triple(b"light", aux);
        assert_eq!(no_site_with(&[r()], &other(SITE_AUX ^ 1)), Ok(Vec::new()));
        assert_eq!(no_site_with(&[], &other(SITE_AUX)).unwrap().len(), 1);
        let odd = triple_aux(b"s", SITE_AUX ^ 1, [0.0; 3]);
        assert_eq!(no_site_with(&[r(), odd], &[]), Ok(Vec::new()));
    }

    #[test]
    fn visibility_with_another_aux_has_no_site() {
        let odd = double_aux(b"v", 0x24, SITE_AUX ^ 1, 1.0);
        assert_eq!(one(&[odd, r()]), Ok(Vec::new()));
    }

    #[test]
    fn non_zero_padding_anywhere_leaves_the_file_siteless() {
        let mut padded = r();
        *padded.last_mut().unwrap() = 0xa5;
        assert_eq!(one(&[padded]), Ok(Vec::new()));
        let mut elsewhere = other_node_with_triple(b"light", SITE_AUX);
        let at = elsewhere.windows(4).position(|w| w == b"DBL3").unwrap();
        elsewhere[at + 43] = 1;
        assert_eq!(no_site_with(&[], &elsewhere), Ok(Vec::new()));
        let mut payload = b"r\0\x20".to_vec();
        payload.extend([0; 24]);
        payload.push(7);
        let long = chunk_zero_pad(b"DBL3", SITE_AUX, &payload, 8);
        assert_eq!(
            one(&[long]),
            Err(()),
            "the decoder enforces the exact payload size"
        );
    }

    #[test]
    fn existing_test_builders_are_siteless() {
        let old = [
            group(b"XFRM", &creation(b"object", None), 4),
            group(
                b"XFRM",
                &[creation(b"other", None), triple(b"r", [0.0; 3])].concat(),
                4,
            ),
        ]
        .concat();
        assert_eq!(sites(&old), Ok(Vec::new()));
    }

    #[test]
    fn the_site_copies_the_files_own_aux_word() {
        let other = 0x5201_0000;
        let children = [
            sited_transform(b"object", &[triple_aux(b"r", other, [0.0; 3])]),
            other_node_with_triple(b"light", other),
        ]
        .concat();
        let decoded = decode(scene(&children)).unwrap();
        let sites = &decoded.source.as_ref().unwrap().sites;
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].aux, other);
    }
}
