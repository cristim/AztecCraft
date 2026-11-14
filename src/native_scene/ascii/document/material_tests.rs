//! Session-material statements (#156): the reader parses them into a side table and the gate
//! attaches it to the candidate. The writer still refuses session materials, so the gate refuses
//! every such file: this slice is inert. `block` writes the nodes the writer will emit.
use super::tests::refused;
use super::*;
use crate::{
    native_scene::{
        MaterialChoice, SessionMaterialId, ascii::decode_inner, session_material::SessionMaterials,
    },
    scene::Primitive,
};

/// `aShape` uses lambert2, `cShape` lambert3, `bShape` the default group; lambert4 is unused.
/// Channels differ so a swap shows.
pub(super) fn scene() -> NativeScene {
    let mut scene = NativeScene::default();
    for name in ["a", "b", "c"] {
        scene.create(Primitive::Cube, name).unwrap();
    }
    let (a, c) = (scene.shapes[0].id, scene.shapes[2].id);
    let new = |color| MaterialChoice::New { color };
    scene
        .assign_session_material(a, new([0.25, 0.5, 1.0]))
        .unwrap();
    scene
        .assign_session_material(c, new([0.75, 0.1, 0.0]))
        .unwrap();
    scene.create_session_material([0.0, 0.2, 0.3]).unwrap();
    scene
}

fn channel(value: f64) -> String {
    (value as f32).to_string()
}

pub(super) fn block(scene: &NativeScene) -> String {
    let table = &scene.session_materials;
    let mut text = String::new();
    for (material, group) in table.materials.iter().zip(&table.groups).skip(1) {
        let [r, g, b] = material.color.map(channel);
        let (m, sg) = (&material.name, &group.name);
        text += &format!(
            "createNode lambert -n \"{m}\";\n\tsetAttr \".c\" -type \"float3\" {r} {g} {b};\n\
             createNode shadingEngine -n \"{sg}\";\n\tsetAttr \".ro\" yes;\n"
        );
    }
    for (material, group) in table.materials.iter().zip(&table.groups).skip(1) {
        let (m, sg) = (&material.name, &group.name);
        text += &format!(
            "connectAttr \"{m}.oc\" \"{sg}.ss\";\n\
             connectAttr \"{sg}.pa\" \":renderPartition.st\" -na;\n\
             connectAttr \"{m}.msg\" \":defaultShaderList1.s\" -na;\n"
        );
    }
    for shape in &scene.shapes {
        let group = match table.assignments.get(&shape.id) {
            Some(index) => table.groups[*index].name.clone(),
            None => ":initialShadingGroup".into(),
        };
        text += &format!("connectAttr \"{}.iog\" \"{group}.dsm\" -na;\n", shape.name);
    }
    text
}

/// The writer's mesh bytes (no links) followed by the material block.
pub(super) fn file(scene: &NativeScene) -> Vec<u8> {
    let mut plain = scene.clone();
    plain.session_materials = SessionMaterials::default();
    let mut bytes = emit_with_links(&plain, DefaultLinks::Omit).unwrap();
    bytes.extend_from_slice(block(scene).as_bytes());
    bytes
}

pub(super) fn read(bytes: &[u8]) -> Result<Decoded, AsciiError> {
    decode_with_uvs(Arc::from(bytes.to_vec()), true)
}

pub(super) fn read_err(bytes: &[u8]) -> ErrorKind {
    read(bytes).map(|_| ()).unwrap_err().kind
}

pub(super) fn attached(bytes: &[u8]) -> NativeScene {
    let Decoded {
        scene,
        uvs,
        materials,
        ..
    } = read(bytes).unwrap();
    candidate_of(&scene, uvs, materials).unwrap()
}

#[test]
fn the_reader_parses_the_block_into_a_table_the_candidate_receives() {
    let model = scene();
    let bytes = file(&model);
    let candidate = attached(&bytes);
    assert_eq!(candidate.session_materials, model.session_materials);
    assert!(candidate.source.is_none());
    // Spelled out so a swapped channel, material or group cannot hide behind an equal model.
    assert_eq!(
        candidate.session_materials()[2].color(),
        [0.75, 0.1f32 as f64, 0.0]
    );
    assert_eq!(candidate.session_materials()[1].color(), [0.25, 0.5, 1.0]);
    assert_eq!(candidate.session_shading_groups()[2].name(), "lambert3SG");
    assert_eq!(
        candidate.session_shading_groups()[2].material(),
        SessionMaterialId::from_index(2)
    );
    let ids: Vec<_> = candidate.shapes.iter().map(|s| s.id).collect();
    assert_eq!(
        candidate
            .session_materials
            .assignments
            .iter()
            .collect::<Vec<_>>(),
        [(&ids[0], &1), (&ids[2], &2)]
    );
    assert_eq!(candidate.session_materials().len(), 4);
    candidate.validate().unwrap();
}

#[test]
fn the_gate_still_refuses_every_material_file_because_the_writer_does() {
    let model = scene();
    let bytes = file(&model);
    assert_eq!(
        refused(&bytes),
        AsciiDocumentError::Ascii(AsciiError {
            kind: ErrorKind::Unsupported,
            offset: None
        })
    );
    // A material in a file with no shape was an error before the reader knew it, and stays one.
    let mut empty = NativeScene::default();
    empty.create_session_material([0.5, 0.5, 0.5]).unwrap();
    let bytes = file(&empty);
    assert!(read(&bytes).unwrap().materials.is_some());
    assert!(matches!(
        refused(&bytes),
        AsciiDocumentError::Ascii(AsciiError {
            kind: ErrorKind::Unsupported,
            ..
        })
    ));
    assert!(decode_inner(Arc::from(file(&model)), false).is_err());
}

#[test]
fn a_block_missing_any_statement_is_refused() {
    let bytes = file(&scene());
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("createNode lambert"))
        .unwrap();
    // Every statement of a material is needed; a shape's link is the gate's business.
    let needed: Vec<usize> = (start..lines.len())
        .filter(|index| !lines[*index].contains(".iog"))
        .collect();
    for &drop in &needed {
        let edited: String = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != drop)
            .map(|(_, line)| *line)
            .collect();
        assert!(read(edited.as_bytes()).is_err(), "kept: {}", lines[drop]);
    }
    assert_eq!(needed.len(), 4 * 3 + 3 * 3);
}
