use super::*;
use crate::native_scene::test_fixtures::cube;
use crate::shading::SURFACE_BASE_RGB;

fn material_of(scene: &NativeScene, shape: NativeId) -> Option<&Lambert> {
    scene.session_material(scene.shape(shape).unwrap())
}

fn assigned_cube() -> (NativeScene, NativeId) {
    let (mut scene, _) = cube();
    let shape = scene.shapes[0].id;
    scene.session_materials.assignments.insert(shape, 0);
    scene.validate().unwrap();
    (scene, shape)
}

#[test]
fn authored_shapes_start_in_the_initial_group_with_the_viewport_base_fill() {
    let (scene, _) = cube();
    let shape = scene.shapes[0].id;
    assert_eq!(
        scene.session_shading_groups()[0].name(),
        "initialShadingGroup"
    );
    assert_eq!(
        material_of(&scene, shape).unwrap().color(),
        SURFACE_BASE_RGB.map(|channel| f64::from(channel) / 255.0)
    );
}

#[test]
fn validate_refuses_each_malformed_table() {
    type Break = fn(&mut NativeScene, NativeId);
    let cases: [(&str, Break, NativeError); 8] = [
        (
            "dangling material",
            |s, _| s.session_materials.groups[0].material = SessionMaterialId(9),
            NativeError::InvalidSessionMaterials,
        ),
        (
            "dangling group",
            |s, shape| {
                s.session_materials.assignments.insert(shape, 9);
            },
            NativeError::InvalidSessionMaterials,
        ),
        (
            "unknown shape",
            |s, _| {
                s.session_materials.assignments.insert(NativeId(9999), 0);
            },
            NativeError::InvalidSessionMaterials,
        ),
        (
            "no materials",
            |s, _| s.session_materials.materials.clear(),
            NativeError::InvalidSessionMaterials,
        ),
        (
            "no groups",
            |s, _| s.session_materials.groups.clear(),
            NativeError::InvalidSessionMaterials,
        ),
        (
            "non-finite",
            |s, _| s.session_materials.materials[0].color[1] = f64::NAN,
            NativeError::NonFinite,
        ),
        (
            "above range",
            |s, _| s.session_materials.materials[0].color[0] = 1.5,
            NativeError::SessionColorOutOfRange,
        ),
        (
            "below range",
            |s, _| s.session_materials.materials[0].color[2] = -0.1,
            NativeError::SessionColorOutOfRange,
        ),
    ];
    for (label, apply, expected) in cases {
        let (mut scene, shape) = assigned_cube();
        apply(&mut scene, shape);
        assert_eq!(scene.validate(), Err(expected), "{label}");
    }
}

#[test]
fn validate_refuses_an_over_cap_or_misaligned_table() {
    let (mut scene, _) = cube();
    let lambert = scene.session_materials.materials[0].clone();
    let group = scene.session_materials.groups[0].clone();
    for index in 1..=MAX_SESSION_MATERIALS {
        scene.session_materials.materials.push(lambert.clone());
        let mut group = group.clone();
        group.material = SessionMaterialId(index);
        scene.session_materials.groups.push(group);
    }
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionMaterials));
    let (mut scene, _) = cube();
    scene.session_materials.materials.push(lambert);
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionMaterials));
    scene.session_materials.groups.push(group.clone());
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionMaterials));
    scene.session_materials.groups[1].material = SessionMaterialId(1);
    scene.validate().unwrap();
    scene.session_materials.groups[0].material = SessionMaterialId(1);
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionMaterials));
}

#[test]
fn the_table_participates_in_equality() {
    let (scene, shape) = assigned_cube();
    let mut other = scene.clone();
    assert_eq!(scene, other);
    other.session_materials.materials[0].color[0] = 0.25;
    assert_ne!(scene, other);
    other = scene.clone();
    other
        .session_materials
        .add([0.1, 0.2, 0.3], std::iter::empty())
        .unwrap();
    assert_ne!(scene, other);
    other = scene.clone();
    other.session_materials.assignments.remove(&shape);
    assert_ne!(scene, other);
}

#[test]
fn removing_an_object_prunes_its_shape_assignment() {
    let (mut scene, shape) = assigned_cube();
    let node = scene.nodes()[0].id();
    scene.remove(node).unwrap();
    assert!(!scene.session_materials.assignments.contains_key(&shape));
    scene.validate().unwrap();
}

/// Names observed in Maya 2025.1 (#177) unless marked: `standardSurface1` on
/// `initialShadingGroup`, then `lambert2`/`lambert2SG` (the unconnected `lambert1` always
/// exists), the number reused after a delete, and `lambert10` beside a transform `lambert9`.
#[test]
fn session_material_names_follow_maya() {
    let (mut scene, _) = cube();
    let names = |scene: &NativeScene| {
        (scene.session_materials().iter().map(Lambert::name))
            .zip(
                scene
                    .session_shading_groups()
                    .iter()
                    .map(ShadingGroup::name),
            )
            .map(|(material, group)| format!("{material}/{group}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&scene), ["standardSurface1/initialShadingGroup"]);
    scene.create_session_material([0.5; 3]).unwrap();
    scene.create_session_material([0.5; 3]).unwrap();
    assert_eq!(
        names(&scene)[1..],
        ["lambert2/lambert2SG", "lambert3/lambert3SG"]
    );

    // Session materials cannot be deleted; restoring the earlier scene stands in for Maya's
    // delete, after which Maya reused lambert2.
    let (mut scene, _) = cube();
    let before = scene.clone();
    scene.create_session_material([0.5; 3]).unwrap();
    scene.restore_authoring(before);
    scene.create_session_material([0.5; 3]).unwrap();
    assert_eq!(names(&scene)[1], "lambert2/lambert2SG");

    let (mut scene, _) = cube();
    scene
        .create(crate::scene::Primitive::Cube, "lambert9")
        .unwrap();
    scene.create_session_material([0.5; 3]).unwrap();
    assert_eq!(names(&scene)[1], "lambert10/lambert10SG");

    // PROVISIONAL: lambert<N>SG names counting toward N is inferred, not observed.
    let (mut scene, _) = cube();
    scene
        .create(crate::scene::Primitive::Cube, "lambert3SG")
        .unwrap();
    let shape = scene.shapes[0].id;
    scene
        .assign_session_material(shape, MaterialChoice::New { color: [0.5; 3] })
        .unwrap();
    assert_eq!(names(&scene)[1], "lambert4/lambert4SG");
    scene.validate().unwrap();
}

#[test]
fn material_numbers_are_u32_and_redo_restores_the_name() {
    assert_eq!(lambert_number("lambert12"), Some(12));
    assert_eq!(lambert_number("lambert12SG"), Some(12));
    for name in [
        "lambert",
        "lambertSG",
        "lambert012",
        "lambert1x",
        "lambert99999999999",
    ] {
        assert_eq!(lambert_number(name), None, "{name}");
    }
    let (mut scene, _) = cube();
    scene
        .create(
            crate::scene::Primitive::Cube,
            &format!("lambert{}", u32::MAX),
        )
        .unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.create_session_material([0.5; 3]),
        Err(NativeError::ResourceLimit)
    );
    assert_eq!(scene, before);

    let (scene, _) = cube();
    let mut history = crate::history::History::new(scene);
    let create = crate::history::Command::CreateSessionMaterial { color: [0.5; 3] };
    history.execute(create).unwrap();
    assert!(history.undo() && history.redo());
    assert_eq!(history.scene().session_materials()[1].name(), "lambert2");
}

mod operations {
    use super::*;
    use crate::history::{Command, EditOutcome, History};
    use crate::native_scene::test_fixtures::shared_shape;

    const RED: [f64; 3] = [1.0, 0.0, 0.0];

    fn id(index: usize) -> SessionMaterialId {
        SessionMaterialId::from_index(index)
    }

    #[test]
    fn create_assign_and_recolor_resolve_through_the_shape() {
        let (mut scene, _) = cube();
        let shape = scene.shapes[0].id;
        let red = scene.create_session_material(RED).unwrap();
        assert_eq!(red, id(1));
        assert_eq!(
            material_of(&scene, shape).unwrap().name(),
            "standardSurface1"
        );
        scene
            .assign_session_material(shape, MaterialChoice::Existing(red))
            .unwrap();
        assert_eq!(material_of(&scene, shape).unwrap().color(), RED);
        scene
            .set_session_material_color(red, [0.0, 1.0, 0.0])
            .unwrap();
        assert_eq!(material_of(&scene, shape).unwrap().color(), [0.0, 1.0, 0.0]);
        scene.validate().unwrap();
    }

    #[test]
    fn refusals_name_the_cause_and_leave_the_scene_unchanged() {
        let (mut scene, _) = cube();
        let shape = scene.shapes[0].id;
        let before = scene.clone();
        let existing = MaterialChoice::Existing;
        let new = |color| MaterialChoice::New { color };
        let results = [
            scene
                .create_session_material([f64::NAN, 0.0, 0.0])
                .map(drop),
            scene.create_session_material([0.0, 2.0, 0.0]).map(drop),
            scene.set_session_material_color(id(7), RED),
            scene.set_session_material_color(id(0), [0.0, f64::INFINITY, 0.0]),
            scene.set_session_material_color(id(0), [-0.5, 0.0, 0.0]),
            scene
                .assign_session_material(shape, existing(id(7)))
                .map(drop),
            scene
                .assign_session_material(shape, new([0.0; 3].map(|_| f64::NAN)))
                .map(drop),
            scene
                .assign_session_material(shape, new([9.0, 0.0, 0.0]))
                .map(drop),
            scene
                .assign_session_material(NativeId(9999), existing(id(0)))
                .map(drop),
        ];
        let expected = [
            NativeError::NonFinite,
            NativeError::SessionColorOutOfRange,
            NativeError::UnknownSessionMaterial(id(7)),
            NativeError::NonFinite,
            NativeError::SessionColorOutOfRange,
            NativeError::UnknownSessionMaterial(id(7)),
            NativeError::NonFinite,
            NativeError::SessionColorOutOfRange,
            NativeError::MissingShape(NativeId(9999)),
        ];
        for (result, expected) in results.into_iter().zip(expected) {
            assert_eq!(result, Err(expected));
        }
        assert_eq!(scene, before);
    }

    #[test]
    fn two_instances_of_one_shape_share_the_assigned_material() {
        let (mut scene, first, second, shape) = shared_shape();
        scene
            .assign_session_material(shape, MaterialChoice::New { color: RED })
            .unwrap();
        for node in [first, second] {
            let transform = scene.node(node).unwrap().transform_id();
            let instance = scene.instance_shape(&[transform, shape]).unwrap();
            assert_eq!(material_of(&scene, instance.id()).unwrap().color(), RED);
        }
    }

    #[test]
    fn history_steps_undo_redo_and_unchanged() {
        let (scene, _) = cube();
        let shape = scene.shapes[0].id;
        let mut history = History::new(scene);
        let Ok(EditOutcome::SessionMaterial(red)) =
            history.execute(Command::CreateSessionMaterial { color: RED })
        else {
            panic!("create returns the new id");
        };
        assert_eq!(red, id(1));
        let assign = Command::AssignSessionMaterial {
            shape,
            material: MaterialChoice::Existing(red),
        };
        assert_eq!(
            history.execute(assign.clone()),
            Ok(EditOutcome::SessionMaterial(red))
        );
        assert_eq!(history.execute(assign), Ok(EditOutcome::Unchanged));
        history
            .execute(Command::SetSessionMaterialColor {
                material: red,
                color: [0.0, 0.0, 1.0],
            })
            .unwrap();
        let color = |history: &History| material_of(history.scene(), shape).unwrap().color();
        assert_eq!(color(&history), [0.0, 0.0, 1.0]);
        assert!(history.undo());
        assert_eq!(color(&history), RED);
        assert!(history.undo());
        assert_eq!(
            material_of(history.scene(), shape).unwrap().name(),
            "standardSurface1"
        );
        assert!(history.undo());
        assert_eq!(history.scene().session_materials().len(), 1);
        assert!(!history.can_undo());
        assert!(history.redo() && history.redo() && history.redo());
        assert_eq!(color(&history), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn assigning_the_initial_material_is_a_no_op_or_clears_the_entry() {
        let (scene, _) = cube();
        let shape = scene.shapes[0].id;
        let mut history = History::new(scene);
        let initial = Command::AssignSessionMaterial {
            shape,
            material: MaterialChoice::Existing(id(0)),
        };
        assert_eq!(history.execute(initial.clone()), Ok(EditOutcome::Unchanged));
        let Ok(EditOutcome::SessionMaterial(red)) =
            history.execute(Command::AssignSessionMaterial {
                shape,
                material: MaterialChoice::New { color: RED },
            })
        else {
            panic!("assign returns the material id");
        };
        assert_eq!(red, id(1));
        assert!(
            history
                .scene()
                .session_materials
                .assignments
                .contains_key(&shape)
        );
        assert!(matches!(
            history.execute(initial),
            Ok(EditOutcome::SessionMaterial(_))
        ));
        assert!(history.scene().session_materials.assignments.is_empty());
    }

    #[test]
    fn the_initial_material_colour_is_locked() {
        let (mut scene, _) = cube();
        let before = scene.clone();
        assert_eq!(
            scene.set_session_material_color(id(0), RED),
            Err(NativeError::InitialSessionMaterialLocked)
        );
        assert_eq!(scene, before);
    }

    #[test]
    fn assign_with_inline_create_is_one_undo_step() {
        let (scene, _) = cube();
        let shape = scene.shapes[0].id;
        let mut history = History::new(scene);
        history
            .execute(Command::AssignSessionMaterial {
                shape,
                material: MaterialChoice::New { color: RED },
            })
            .unwrap();
        assert_eq!(material_of(history.scene(), shape).unwrap().color(), RED);
        assert!(history.undo());
        assert!(!history.can_undo());
        assert_eq!(history.scene().session_materials().len(), 1);
    }
}
