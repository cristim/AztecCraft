use super::*;
use crate::native_scene::binary::{decode, tests::appearance_fixture as wire_fixture};

#[test]
fn diffuse_is_one_authoritative_word_for_two_shading_groups_sharing_a_shader() {
    let bytes: Arc<[u8]> = wire_fixture::appearance_specimen_with_users(2).into();
    let mut scene = decode(bytes.clone()).unwrap();
    assert_eq!(scene.shapes().len(), 2);
    assert_eq!(
        scene
            .source
            .as_ref()
            .unwrap()
            .appearance
            .as_ref()
            .unwrap()
            .materials
            .len(),
        2
    );
    assert_eq!(scene.materials.len(), 1);
    let inspected = crate::maya_binary::container::inspect(bytes.clone()).unwrap();
    let facts = crate::maya_binary::semantic::decode_records(&inspected).unwrap();
    let shader_group = scene
        .source
        .as_ref()
        .unwrap()
        .appearance
        .as_ref()
        .unwrap()
        .diffuse[0]
        .shader;
    let shader_record = facts
        .records
        .iter()
        .position(|record| record.group == shader_group && matches!(record.data, Data::Create(_)))
        .unwrap();
    assert_ne!(
        shader_group, shader_record,
        "fixture must distinguish owner identity spaces"
    );
    let first = scene.shapes()[0].id();
    let second = scene.shapes()[1].id();
    assert_eq!(scene.material_for_shape(first).unwrap().shared_users, 2);
    let readiness = scene.appearance_readiness();
    let before = scene.clone();
    let value = 1.0000000000000004;
    scene.set_material_diffuse(first, value).unwrap();
    assert_ne!(scene, before);
    for shape in [first, second] {
        let material = scene.material_for_shape(shape).unwrap();
        assert_eq!(material.diffuse.to_bits(), value.to_bits());
        assert!(material.editing_available && !material.rendering_available);
    }
    assert!(Arc::ptr_eq(&bytes, &scene.source.as_ref().unwrap().bytes));
    assert_eq!(scene.appearance_readiness(), readiness);
    let output = super::super::encode(&scene).unwrap();
    let start = bytes
        .windows(4)
        .position(|word| word == b"dc\0\x24")
        .unwrap()
        + 4;
    assert_eq!(&output[..start], &bytes[..start]);
    assert_eq!(&output[start..start + 8], &value.to_be_bytes());
    assert_eq!(&output[start + 8..], &bytes[start + 8..]);
    let reopened = decode(output.into()).unwrap();
    assert_eq!(reopened.materials.len(), 1);
    assert_eq!(
        reopened
            .material_for_shape(second)
            .unwrap()
            .diffuse
            .to_bits(),
        value.to_bits()
    );
    assert_eq!(
        scene.source.as_ref().unwrap().appearance,
        reopened.source.as_ref().unwrap().appearance
    );
    scene.set_material_diffuse(second, 0.8).unwrap();
    assert_eq!(scene, before);
    assert!(super::super::encode(&scene).unwrap() == bytes.as_ref());
}

#[test]
fn unsupported_other_shader_consumers_refuse_edits_without_changing_readiness() {
    let mut scene = decode(wire_fixture::appearance_specimen_with_users(3).into()).unwrap();
    assert_eq!(scene.shapes().len(), 3);
    let readiness = scene.appearance_readiness();
    assert_eq!(readiness.checked_source_bindings, 2);
    assert_eq!(readiness.unavailable, 1);
    assert!(scene.materials.is_empty());
    let before = scene.clone();
    for shape in scene
        .shapes()
        .iter()
        .map(|shape| shape.id())
        .collect::<Vec<_>>()
    {
        assert_eq!(
            scene.material_for_shape(shape),
            Err(crate::native_scene::NativeError::UnsupportedEdit)
        );
        assert_eq!(
            scene.set_material_diffuse(shape, 0.5),
            Err(crate::native_scene::NativeError::UnsupportedEdit)
        );
        assert_eq!(scene, before);
    }
    assert_eq!(scene.appearance_readiness(), readiness);
}

#[test]
fn diffuse_edits_preserve_raw_zero_words_unusual_source_and_failure_baselines() {
    let original = wire_fixture::appearance_specimen();
    let start = original
        .windows(4)
        .position(|word| word == b"dc\0\x24")
        .unwrap()
        + 4;
    for source_value in [-0.0f64, 0.0, -0.25, 2.5, 1.0000000000000004] {
        let mut bytes = original.clone();
        bytes[start..start + 8].copy_from_slice(&source_value.to_be_bytes());
        let mut scene = decode(bytes.clone().into()).unwrap();
        let shape = scene.shapes()[0].id();
        assert_eq!(
            scene.material_for_shape(shape).unwrap().diffuse.to_bits(),
            source_value.to_bits()
        );
        assert!(super::super::encode(&scene).unwrap() == bytes);
        let before = scene.clone();
        for value in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(scene.set_material_diffuse(shape, value).is_err());
            assert_eq!(scene, before);
        }
        for value in [0.0f64, -0.0, 3.25] {
            scene.set_material_diffuse(shape, value).unwrap();
            assert_eq!(
                scene.material_for_shape(shape).unwrap().diffuse.to_bits(),
                value.to_bits()
            );
            assert_eq!(
                scene.materials[0].replaced,
                value.to_bits() != source_value.to_bits()
            );
        }
        if source_value >= 0.0 {
            scene.set_material_diffuse(shape, source_value).unwrap();
            assert_eq!(scene, before);
            assert!(super::super::encode(&scene).unwrap() == bytes);
        }
    }
    let sourcefree = crate::native_scene::NativeScene::default();
    assert!(sourcefree.materials.is_empty());
}

#[test]
fn material_history_noop_revert_and_save_snapshot_use_the_same_scene() {
    use crate::history::{Command, EditOutcome, History};
    let source = wire_fixture::appearance_specimen_with_users(2);
    let mut history = History::new(decode(source.clone().into()).unwrap());
    let shape = history.scene().shapes()[0].id();
    let command = |value| Command::SetMaterialDiffuse { shape, value };
    assert_eq!(history.execute(command(0.8)), Ok(EditOutcome::Unchanged));
    assert!(!history.can_undo());
    assert_eq!(history.execute(command(0.5)), Ok(EditOutcome::Changed));
    let saved = history.scene().clone();
    let encoded = super::super::encode(&saved).unwrap();
    assert!(history.undo());
    assert_ne!(history.scene(), &saved);
    assert!(history.can_redo());
    assert_eq!(history.execute(command(0.8)), Ok(EditOutcome::Unchanged));
    assert!(history.can_redo());
    assert!(history.execute(command(-1.0)).is_err());
    assert!(history.can_redo());
    assert!(history.redo());
    assert_eq!(history.scene(), &saved);
    assert!(super::super::encode(history.scene()).unwrap() == encoded);
    assert_eq!(history.execute(command(0.8)), Ok(EditOutcome::Changed));
    assert!(super::super::encode(history.scene()).unwrap() == source);
    assert!(history.undo());
    assert_eq!(history.scene(), &saved);
}

#[test]
fn diffuse_signed_zero_changes_history_while_both_words_have_replacement_intent() {
    use crate::history::{Command, EditOutcome, History};
    let bytes = wire_fixture::appearance_specimen();
    let mut history = History::new(decode(bytes.clone().into()).unwrap());
    let shape = history.scene().shapes()[0].id();
    let command = |value| Command::SetMaterialDiffuse { shape, value };
    assert_eq!(history.execute(command(0.0)), Ok(EditOutcome::Changed));
    let positive = history.scene().clone();
    assert!(positive.materials[0].replaced);
    assert_eq!(history.execute(command(-0.0)), Ok(EditOutcome::Changed));
    let negative = history.scene().clone();
    assert!(negative.materials[0].replaced);
    assert_ne!(positive, negative);
    let output = super::super::encode(&negative).unwrap();
    let start = bytes
        .windows(4)
        .position(|word| word == b"dc\0\x24")
        .unwrap()
        + 4;
    assert_eq!(&output[start..start + 8], &(-0.0f64).to_be_bytes());
    assert!(history.undo());
    assert_eq!(history.scene(), &positive);
    assert!(history.redo());
    assert_eq!(history.scene(), &negative);
    assert_eq!(history.execute(command(-0.0)), Ok(EditOutcome::Unchanged));
}

#[test]
fn diffuse_requires_the_saved_scalar_and_refuses_connected_or_selected_overrides() {
    use crate::native_scene::binary::tests::wire_fixture as wire;
    let original = wire_fixture::appearance_specimen();
    let container = crate::maya_binary::container::inspect(original.clone().into()).unwrap();
    let scalar = container
        .chunks()
        .iter()
        .find(|chunk| {
            chunk.tag() == *b"DBLE"
                && container.source()[chunk.payload_range()].starts_with(b"dc\0")
        })
        .unwrap();
    let group_start = container
        .chunk(scalar.parent().unwrap())
        .unwrap()
        .header_range()
        .start;
    let removed = scalar.padding_range().end - scalar.header_range().start;
    let mut missing = original.clone();
    let group_size = u64::from_be_bytes(
        missing[group_start + 8..group_start + 16]
            .try_into()
            .unwrap(),
    );
    missing[group_start + 8..group_start + 16]
        .copy_from_slice(&(group_size - removed as u64).to_be_bytes());
    missing.drain(scalar.header_range().start..scalar.padding_range().end);
    let root_size = (missing.len() - 16) as u64;
    missing[8..16].copy_from_slice(&root_size.to_be_bytes());
    let selected = wire::group(
        b"SLCT",
        &[
            wire::chunk(b"SLCT", b":shader", 8),
            wire::chunk(
                b"DBLE",
                &[b"dc\0\x24".as_slice(), &0.5f64.to_be_bytes()].concat(),
                8,
            ),
        ]
        .concat(),
        4,
    );
    let selected = wire::group(b"Maya", &[original[20..].to_vec(), selected].concat(), 1);
    let connected = wire_fixture::appearance_specimen_with_connections(&connection(
        0,
        b"texture.oc",
        b"shader.dc",
    ));
    for bytes in [missing, selected, connected] {
        let mut scene = decode(bytes.into()).unwrap();
        let shape = scene.shapes()[0].id();
        let before = scene.clone();
        assert_eq!(
            scene.material_for_shape(shape),
            Err(crate::native_scene::NativeError::UnsupportedEdit)
        );
        assert!(scene.set_material_diffuse(shape, 0.25).is_err());
        assert_eq!(scene, before);
    }
}

#[test]
#[ignore = "requires explicit private manifest/appearance ledger and separate original SHA precheck"]
fn private_viewer_bindings_match_independent_owner_link_and_uv_ranges() {
    use crate::native_scene::binary::tests::read;
    use std::path::Path;
    let manifest =
        std::env::var_os("AZTECCRAFT_MB_FIXTURE_MANIFEST").expect("explicit manifest required");
    let manifest = Path::new(&manifest);
    let directory = manifest.parent().unwrap().canonicalize().unwrap();
    let entries: serde_json::Value = serde_json::from_slice(&read(manifest)).unwrap();
    let path = directory
        .join(entries[0]["relative_path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    assert!(path.starts_with(&directory));
    let bytes: Arc<[u8]> = read(&path).into();
    let ledger = std::env::var_os("AZTECCRAFT_APPEARANCE_LEDGER")
        .expect("explicit independent appearance ledger required");
    let ledger: serde_json::Value = serde_json::from_slice(&read(Path::new(&ledger))).unwrap();
    let scene = decode(bytes.clone()).unwrap();
    let container = crate::maya_binary::container::inspect(bytes.clone()).unwrap();
    let metadata = scene.source.as_ref().unwrap().appearance.as_ref().unwrap();
    let number = |value: &serde_json::Value| usize::try_from(value.as_u64().unwrap()).unwrap();
    let range = |value: &serde_json::Value| number(&value[0])..number(&value[1]);
    let offset = |index| container.chunk(index).unwrap().header_range().start;
    let owners = ledger["viewer"]["owners"].as_array().unwrap();
    let connections = ledger["viewer"]["connections"].as_array().unwrap();
    let link = |span: &SourceSpan, source: Option<usize>, destination: Option<usize>, mode: u8| {
        let record = connections
            .iter()
            .find(|record| number(&record["offset"]) + 16 == span.leaf.start)
            .unwrap();
        assert_eq!(
            number(&record["mode"]),
            mode as usize,
            "independent connection mode"
        );
        for (endpoint, expected) in record["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .zip([source, destination])
        {
            assert!(
                endpoint["owner"].as_u64().map(|value| value as usize) == expected,
                "independent endpoint owner mismatch"
            );
        }
    };
    assert_eq!(metadata.shapes.len(), 3);
    for (ordinal, entry) in metadata.shapes.iter().enumerate() {
        let binding = entry.binding.as_ref().unwrap();
        let shape = offset(entry.source_group);
        let owner = owners
            .iter()
            .find(|owner| number(&owner["group"]) == shape)
            .unwrap();
        let fields = owner["fields"].as_array().unwrap();
        let field = |key| fields.iter().find(|field| field["key"] == key).unwrap();
        assert!(
            binding.component.value == range(&field("iog[0].og[0].gcl")["value_span"]),
            "component range differs at shape {ordinal}"
        );
        let explicit = fields.iter().find(|field| field["tag"] == "FLT2").unwrap();
        assert!(
            binding.explicit_uv.value == range(&explicit["value_span"]),
            "explicit UV range differs at shape {ordinal}"
        );
        for (span, key) in [
            (&binding.explicit_uv_name, "uvst[0].uvsn"),
            (&binding.active_uv_name, "cuvs"),
        ] {
            let expected = range(&field(key)["value_span"]);
            assert!(
                span.value == (expected.start..expected.end - 1),
                "UV name range differs at shape {ordinal}"
            );
        }
        let mesh = ledger["viewer"]["meshes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|mesh| number(&mesh["chunkOffset"]) + 16 == binding.packed_uv.leaf.start)
            .unwrap();
        let base = number(&mesh["chunkOffset"]) + 16;
        let set = &mesh["sets"][0];
        for (actual, key) in [
            (&binding.packed_uv.value, "points"),
            (&binding.corner_ids.value, "ids"),
            (&binding.uv_name.value, "nameRange"),
        ] {
            let expected = range(&set[key]);
            assert!(
                *actual == (base + expected.start..base + expected.end),
                "packed range differs at shape {ordinal}"
            );
        }
        assert!(
            bytes[binding.explicit_uv.value.clone()] == bytes[binding.packed_uv.value.clone()],
            "UV words differ at shape {ordinal}"
        );
        let material = &metadata.materials[binding.material];
        let [surface, shader, texture, placement] = material.owners.map(offset);
        let group_id = offset(binding.group_id);
        link(&binding.membership_link, Some(shape), Some(surface), 1);
        link(&binding.group_id_link, Some(group_id), Some(shape), 0);
        link(&binding.group_link, Some(group_id), Some(surface), 1);
        link(&binding.group_color_link, Some(surface), Some(shape), 0);
        link(&material.shader_link, Some(shader), Some(surface), 0);
        link(&material.texture_link, Some(texture), Some(shader), 0);
        for span in &material.placement_links {
            link(span, Some(placement), Some(texture), 0);
        }
        for span in &material.color_links {
            link(span, None, Some(texture), 0);
        }
        let cm = ledger["fixtures"][0]["default_display_contexts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|context| context["context"] == ":defaultColorMgtGlobals")
            .unwrap();
        for input in &material.inputs {
            let InputEvidence::Saved { property, span } = input else {
                continue;
            };
            let (owner, key) = match property {
                InputProperty::TexturePath => (Some(texture), "ftn"),
                InputProperty::ColorSpace => (Some(texture), "cs"),
                InputProperty::Diffuse => (Some(shader), "dc"),
                InputProperty::Ambient => (Some(shader), "ambc"),
                InputProperty::Specular => (Some(shader), "sc"),
                InputProperty::Reflectivity => (Some(shader), "rfl"),
                InputProperty::ColorConfigurationEnabled => (None, "cfe"),
                InputProperty::ColorConfiguration => (None, "cfp"),
                InputProperty::WorkingSpace => (None, "wsn"),
                InputProperty::ShadingGroupHistoryInterest2026 => (Some(surface), "ihi"),
                InputProperty::ShadingGroupRenderableOnlySet2026 => (Some(surface), "ro"),
                _ => panic!("unexpected saved input property"),
            };
            let fields = owner
                .map(|owner| {
                    &owners
                        .iter()
                        .find(|entry| number(&entry["group"]) == owner)
                        .unwrap()["fields"]
                })
                .unwrap_or(&cm["fields"]);
            let expected = fields
                .as_array()
                .unwrap()
                .iter()
                .find(|field| field["key"] == key)
                .unwrap();
            assert!(
                span.value == range(&expected["value_span"]),
                "saved input source range differs at shape {ordinal}"
            );
            if *property == InputProperty::Diffuse {
                let expected_range = range(&expected["value_span"]);
                let word: [u8; 8] = bytes[expected_range.clone()].try_into().unwrap();
                let value = scene.material_for_shape(entry.shape).unwrap();
                assert!(
                    value.diffuse.to_bits() == f64::from_be_bytes(word).to_bits(),
                    "diffuse source word differs at shape {ordinal}"
                );
                assert!(
                    metadata.diffuse.iter().any(|binding| {
                        binding.shader == material.owners[1]
                            && binding.span.value == expected_range
                            && binding.shapes.contains(&entry.shape)
                    }),
                    "diffuse binding owner/range differs at shape {ordinal}"
                );
            }
        }
        for message in &material.message_links {
            let record = connections
                .iter()
                .find(|record| number(&record["offset"]) + 16 == message.span.leaf.start)
                .unwrap();
            assert!(
                message.source == range(&record["endpoints"][0]["span"]),
                "message source span differs at shape {ordinal}"
            );
            assert!(
                message.destination == range(&record["endpoints"][1]["span"]),
                "message destination span differs at shape {ordinal}"
            );
            assert_eq!(message.mode as usize, number(&record["mode"]));
        }
    }
}

fn replace(bytes: &mut [u8], old: &[u8], new: &[u8]) {
    assert_eq!(old.len(), new.len());
    let positions: Vec<_> = bytes
        .windows(old.len())
        .enumerate()
        .filter_map(|(index, value)| (value == old).then_some(index))
        .collect();
    assert_eq!(
        positions.len(),
        1,
        "synthetic patch must identify one source record"
    );
    bytes[positions[0]..positions[0] + old.len()].copy_from_slice(new);
}

fn unavailable(bytes: Vec<u8>, expected: AppearanceIssue) {
    let scene = decode(bytes.into()).unwrap();
    assert_eq!(scene.shapes().len(), 1);
    let binding = &scene
        .source
        .as_ref()
        .unwrap()
        .appearance
        .as_ref()
        .unwrap()
        .shapes[0]
        .binding;
    assert_eq!(binding, &Err(expected));
}

#[test]
fn a_material_on_a_mesh_with_an_empty_uv_set_is_unavailable_not_an_error() {
    // The file still carries a three-point uvsp array, so the length check at the
    // explicit array (before the later count/2 hint comparison) reports the mismatch.
    let bytes = wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
        empty_mesh_uv: true,
        ..Default::default()
    });
    unavailable(bytes, AppearanceIssue::UvMismatch);
}

fn connection(mode: u8, source: &[u8], destination: &[u8]) -> Vec<u8> {
    crate::native_scene::binary::tests::wire_fixture::chunk(
        b"CWFL",
        &[&[mode], source, &[0], destination, &[0]].concat(),
        8,
    )
}

#[test]
fn checked_source_binding_retains_message_links_and_exact_uv_words() {
    let bytes: Arc<[u8]> = wire_fixture::appearance_specimen().into();
    let scene = decode(bytes.clone()).unwrap();
    let summary = scene.appearance_readiness();
    assert_eq!(summary.state, AppearanceState::CheckedViewerSource);
    assert_eq!(summary.checked_source_bindings, 1);
    assert!(summary.issues.evaluation_unavailable);
    let source = scene.source.as_ref().unwrap();
    assert!(Arc::ptr_eq(&source.bytes, &bytes));
    let metadata = source.appearance.as_ref().unwrap();
    let surface = metadata.shapes[0].binding.as_ref().unwrap();
    assert_eq!(surface.faces, 0..2);
    assert_eq!(
        &bytes[surface.explicit_uv.value.clone()],
        &bytes[surface.packed_uv.value.clone()]
    );
    let ids: Vec<_> = bytes[surface.corner_ids.value.clone()]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_be_bytes(*word))
        .collect();
    assert_eq!(ids, [0, 1, 2, 3, 1, 2]);
    let material = &metadata.materials[surface.material];
    assert_eq!(material.message_links.len(), 2);
    assert!(
        material
            .message_links
            .iter()
            .any(|link| &bytes[link.source.clone()] == b"surface.pa")
    );
    assert!(
        material
            .message_links
            .iter()
            .any(|link| &bytes[link.source.clone()] == b"shader.msg")
    );
}

#[test]
fn source_owners_modes_uv_words_and_required_links_are_decisive() {
    let mut bytes = wire_fixture::appearance_specimen();
    replace(
        &mut bytes,
        b"identity.msg\0surface.gn",
        b"othergid.msg\0surface.gn",
    );
    unavailable(bytes, AppearanceIssue::UnsupportedGraph);

    let mut bytes = wire_fixture::appearance_specimen();
    replace(
        &mut bytes,
        b"\x01shape.iog.og[0]\0surface.dsm",
        b"\x00shape.iog.og[0]\0surface.dsm",
    );
    unavailable(bytes, AppearanceIssue::UnsupportedGraph);

    let mut bytes = wire_fixture::appearance_specimen();
    replace(&mut bytes, b"uvst[0].uvsn\0\x24", b"uvst[0].uvsn\0\x20");
    unavailable(bytes, AppearanceIssue::UnsupportedSettings);

    let mut bytes = wire_fixture::appearance_specimen();
    let start = bytes
        .windows(24)
        .position(|value| value.starts_with(b"uvst[0].uvsp[0:3]\0\x20"))
        .unwrap();
    let value_start = start + b"uvst[0].uvsp[0:3]\0\x20".len();
    bytes[value_start..value_start + 4].copy_from_slice(&0.5f32.to_be_bytes());
    unavailable(bytes, AppearanceIssue::UvMismatch);
}

#[test]
fn association_message_allowance_does_not_admit_unknown_value_outputs() {
    let mut bytes = wire_fixture::appearance_specimen();
    replace(
        &mut bytes,
        b"surface.pa\0:partition.st",
        b"surface.zz\0:partition.st",
    );
    unavailable(bytes, AppearanceIssue::UnsupportedGraph);
    let mut bytes = wire_fixture::appearance_specimen();
    replace(
        &mut bytes,
        b"shader.msg\0:materials.s",
        b"shader.abc\0:materials.s",
    );
    unavailable(bytes, AppearanceIssue::UnsupportedGraph);

    let mut bytes = wire_fixture::appearance_specimen();
    replace(&mut bytes, b"RPHO", b"RLLK");
    unavailable(bytes, AppearanceIssue::UnsupportedShader);
}

#[test]
fn overlapping_membership_competing_inputs_and_cycles_do_not_choose_a_winner() {
    unavailable(
        wire_fixture::appearance_specimen_with_connections(&connection(
            1,
            b"shape.iog.og[0]",
            b"surface.dsm",
        )),
        AppearanceIssue::OverlappingMembership,
    );
    unavailable(
        wire_fixture::appearance_specimen_with_connections(&connection(
            0,
            b"texture.oc",
            b"shader.c",
        )),
        AppearanceIssue::UnsupportedGraph,
    );
    unavailable(
        wire_fixture::appearance_specimen_with_connections(&connection(
            0,
            b"shader.oc",
            b"placement.uv",
        )),
        AppearanceIssue::UnsupportedGraph,
    );
}

#[test]
fn selected_created_material_overrides_cannot_remain_checked() {
    use crate::native_scene::binary::tests::wire_fixture as wire;

    for target in [
        b":surface".as_slice(),
        b":shader",
        b":texture",
        b":placement",
        b":|unresolved",
    ] {
        let bytes = wire_fixture::appearance_specimen();
        let selected = wire::group(
            b"SLCT",
            &[
                wire::chunk(b"SLCT", target, 8),
                wire::chunk(
                    b"DBLE",
                    &[b"dc\0\x24".as_slice(), &0.25f64.to_be_bytes()].concat(),
                    8,
                ),
            ]
            .concat(),
            4,
        );
        unavailable(
            wire::group(b"Maya", &[bytes[20..].to_vec(), selected].concat(), 1),
            AppearanceIssue::UnsupportedGraph,
        );
    }
}

#[test]
fn other_membership_slots_cannot_remain_checked() {
    for (source, destination) in [
        (b"shape.iog".as_slice(), b":otherSet.dsm".as_slice()),
        (b"shape.iog[0]", b":otherSet.dsm[0]"),
        (b"shape.iog[0].og[1]", b":otherSet.dsm[0]"),
    ] {
        unavailable(
            wire_fixture::appearance_specimen_with_connections(&connection(1, source, destination)),
            AppearanceIssue::OverlappingMembership,
        );
    }
}

#[test]
fn shading_group_inputs_and_saved_state_cannot_remain_checked() {
    for key in [b"surface.ds".as_slice(), b"surface.vs"] {
        unavailable(
            wire_fixture::appearance_specimen_with_connections(&connection(0, b"othergid.id", key)),
            AppearanceIssue::UnsupportedGraph,
        );
    }
    let saved = crate::native_scene::binary::tests::wire_fixture::chunk(
        b"DBLE",
        &[b"ds\0\x20".as_slice(), &1f64.to_be_bytes()].concat(),
        8,
    );
    unavailable(
        wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
            surface: &saved,
            ..Default::default()
        }),
        AppearanceIssue::UnsupportedSettings,
    );
}

#[test]
fn shading_group_inherited_state_has_checked_typed_provenance_without_evaluation() {
    let state = |key: &[u8], kind, value: f64| {
        crate::native_scene::binary::tests::wire_fixture::chunk(
            b"DBLE",
            &[key, &[0, kind], &value.to_be_bytes()].concat(),
            8,
        )
    };
    let valid = [state(b"ihi", 0x20, 2.0), state(b"ro", 0x20, 1.0)].concat();
    let scene = decode(
        wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
            surface: &valid,
            ..Default::default()
        })
        .into(),
    )
    .unwrap();
    assert_eq!(scene.appearance_readiness().checked_source_bindings, 1);
    let metadata = scene.source.as_ref().unwrap().appearance.as_ref().unwrap();
    assert!(metadata.materials[0].inputs.iter().any(|input| matches!(
        input,
        InputEvidence::Saved {
            property: InputProperty::ShadingGroupHistoryInterest2026,
            ..
        }
    )));
    assert!(metadata.materials[0].inputs.iter().any(|input| matches!(
        input,
        InputEvidence::Saved {
            property: InputProperty::ShadingGroupRenderableOnlySet2026,
            ..
        }
    )));
    for value in [
        state(b"ihi", 0x20, 256.0),
        state(b"ihi", 0x20, 0.5),
        state(b"ro", 0x20, 2.0),
        state(b"ro", 0x20, f64::NAN),
        state(b"ro", 0x24, 1.0),
        [state(b"ro", 0x20, 0.0), state(b"ro", 0x20, 1.0)].concat(),
    ] {
        unavailable(
            wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
                surface: &value,
                ..Default::default()
            }),
            AppearanceIssue::UnsupportedSettings,
        );
    }
}

#[test]
fn unknown_component_variants_stay_unavailable() {
    let unknown = component_record(&[1, u32::from_be_bytes(*b"CMDF"), 2, 0, 0, 1, 1]);
    unavailable(
        wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
            shape: &unknown,
            ..Default::default()
        }),
        AppearanceIssue::UnsupportedSettings,
    );
}

#[test]
fn short_unknown_component_discriminators_are_unavailable() {
    for words in [&[2][..], &[1, u32::from_be_bytes(*b"OTHR")][..]] {
        let record = component_record(words);
        unavailable(
            wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
                shape: &record,
                ..Default::default()
            }),
            AppearanceIssue::UnsupportedSettings,
        );
    }
}

#[test]
fn supported_component_discriminator_prefixes_require_their_next_word() {
    for words in [
        &[][..],
        &[1][..],
        &[1, u32::from_be_bytes(*b"CMDF")][..],
        &[1, u32::from_be_bytes(*b"CMDF"), 1][..],
        &[1, u32::from_be_bytes(*b"CMDF"), 1, 0][..],
    ] {
        let record = component_record(words);
        assert_eq!(
            decode(
                wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
                    shape: &record,
                    ..Default::default()
                })
                .into()
            )
            .unwrap_err()
            .kind(),
            ErrorKind::Malformed
        );
    }
}

fn component_record(words: &[u32]) -> Vec<u8> {
    let mut payload = b"iog[0].og[0].gcl\0\x20".to_vec();
    for word in words {
        payload.extend(word.to_be_bytes());
    }
    crate::native_scene::binary::tests::wire_fixture::chunk(b"CMP#", &payload, 8)
}

#[test]
fn unavailable_partial_coverage_cannot_mask_later_known_out_of_bounds_component() {
    let invalid = component_record(&[1, u32::from_be_bytes(*b"CMDF"), 1, 0, 2]);
    let mut bytes = wire_fixture::appearance_specimen_with_fields(wire_fixture::Fields {
        shape: &invalid,
        ..Default::default()
    });
    let marker = b"iog[0].og[0].gcl\0\x20";
    let first = bytes
        .windows(marker.len())
        .position(|value| value == marker)
        .unwrap()
        + marker.len();
    bytes[first + 16..first + 20].copy_from_slice(&0u32.to_be_bytes());
    assert_eq!(
        decode(bytes.into()).unwrap_err().kind(),
        ErrorKind::Malformed
    );
}

#[test]
fn opaque_appearance_payloads_are_unavailable_without_invented_key_framing() {
    let opaque =
        crate::native_scene::binary::tests::wire_fixture::chunk(b"OPAQ", b"no-cstring-framing", 8);
    for fields in [
        wire_fixture::Fields {
            file: &opaque,
            ..Default::default()
        },
        wire_fixture::Fields {
            shader: &opaque,
            ..Default::default()
        },
        wire_fixture::Fields {
            color_management: &opaque,
            ..Default::default()
        },
    ] {
        unavailable(
            wire_fixture::appearance_specimen_with_fields(fields),
            AppearanceIssue::UnsupportedSettings,
        );
    }
}

#[test]
fn owner_table_capacity_counts_toward_the_local_metadata_boundary() {
    use crate::native_scene::binary::{decode_with_retained_bytes, tests::wire_fixture};

    let bytes = wire_fixture::scene(&wire_fixture::group(
        b"XFRM",
        &wire_fixture::creation(b"object", None),
        4,
    ));
    let container = crate::maya_binary::container::inspect(bytes.clone()).unwrap();
    let facts = crate::maya_binary::semantic::decode_records(&container).unwrap();
    let mut budget = Budget::new(0).unwrap();
    let owner_bytes = budget.vector::<usize>(1).unwrap().capacity() * size_of::<usize>();
    let graph_bytes = budget
        .vector::<usize>(facts.records.len())
        .unwrap()
        .capacity()
        * size_of::<usize>()
        + budget
            .vector::<bool>(facts.records.len())
            .unwrap()
            .capacity()
            * size_of::<bool>();
    let promoted_bytes = facts.records.capacity()
        * (size_of::<Range<usize>>() + 2 * size_of::<crate::maya_binary::semantic::Words>());
    let retained = MAX_METADATA
        - owner_bytes
        - graph_bytes
        - promoted_bytes
        - size_of::<SourceAppearance>()
        - 2 * size_of::<usize>();
    assert!(decode_with_retained_bytes(bytes.clone(), 0, retained).is_ok());
    assert_eq!(
        decode_with_retained_bytes(bytes, 0, retained + 1)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn metadata_budget_and_work_checks_have_exact_boundaries() {
    let mut budget = Budget::new(0).unwrap();
    let mut meter = Meter {
        budget: &mut budget,
        bytes: MAX_METADATA - 1,
        steps: MAX_STEPS - 1,
    };
    assert!(meter.charge(1).is_ok());
    assert_eq!(
        meter.charge(1).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert!(meter.step(1).is_ok());
    assert_eq!(meter.step(1).unwrap_err().kind(), ErrorKind::ResourceLimit);
    assert_eq!(
        meter.vector::<u64>(1).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    meter.bytes = usize::MAX;
    assert_eq!(
        meter.charge(1).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    meter.steps = usize::MAX;
    assert_eq!(meter.step(1).unwrap_err().kind(), ErrorKind::ResourceLimit);
}

#[test]
fn unavailable_membership_cannot_hide_later_known_malformed_framing() {
    let mut missing = wire_fixture::appearance_specimen();
    replace(&mut missing, b"iog[0].og[0].gcl", b"iog[0].og[1].gcl");
    unavailable(missing.clone(), AppearanceIssue::MissingMembership);
    let mut malformed_uv = missing.clone();
    replace(
        &mut malformed_uv,
        b"uvst[0].uvsp[0:3]",
        b"uvst[0].uvsp[0:4]",
    );
    assert_eq!(
        decode(malformed_uv.into()).unwrap_err().kind(),
        ErrorKind::Malformed
    );
    let mut malformed_file = missing.clone();
    replace(
        &mut malformed_file,
        b"ftn\0\x20test.png\0",
        b"ftn\0\x20test.pngx",
    );
    assert_eq!(
        decode(malformed_file.into()).unwrap_err().kind(),
        ErrorKind::Malformed
    );
    replace(
        &mut missing,
        b"uvst[0].uvsp[0:3]\0\x20",
        b"uvst[0].uvsp[0:3]\0\x21",
    );
    unavailable(missing, AppearanceIssue::MissingMembership);
}

#[test]
fn session_material_edits_refuse_sourced_scenes_and_keep_phong_diffuse_editable() {
    use crate::native_scene::{MaterialChoice, NativeError, SessionMaterialId};
    let bytes: Arc<[u8]> = wire_fixture::appearance_specimen_with_users(2).into();
    let mut scene = decode(bytes).unwrap();
    let shape = scene.shapes[0].id;
    let before = scene.clone();
    assert_eq!(
        scene.create_session_material([1.0, 0.0, 0.0]),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(
        scene.assign_session_material(shape, MaterialChoice::New { color: [1.0; 3] }),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(
        scene.assign_session_material(
            shape,
            MaterialChoice::Existing(SessionMaterialId::from_index(0))
        ),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(
        scene.set_session_material_color(SessionMaterialId::from_index(0), [0.0; 3]),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, before);
    assert!(
        scene
            .session_material(scene.shape(shape).unwrap())
            .is_none()
    );
    scene.set_material_diffuse(shape, 0.25).unwrap();
    assert_ne!(scene, before);
    assert!(crate::native_scene::binary::encode(&scene).is_ok());
}
