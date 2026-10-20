use azteccraft::history::{Command, EditOutcome, History, UNDO_LIMIT};
use azteccraft::native_scene::{NativeError, NativeScene, NativeTransform};
use azteccraft::scene::{NodeId, Primitive, SceneError};

fn create(history: &mut History, name: &str) -> NodeId {
    match history
        .execute(Command::Create {
            primitive: Primitive::Cube,
            options: None,
            name: name.into(),
        })
        .unwrap()
    {
        EditOutcome::Created(id) => id,
        outcome => panic!("unexpected outcome: {outcome:?}"),
    }
}

#[test]
fn undo_and_redo_restore_every_authoring_state_in_order() {
    let mut history = History::default();
    let mut states = vec![history.scene().nodes().to_vec()];
    let cube = create(&mut history, "cube");
    states.push(history.scene().nodes().to_vec());
    let plane = match history
        .execute(Command::Create {
            primitive: Primitive::Plane,
            options: None,
            name: "ground".into(),
        })
        .unwrap()
    {
        EditOutcome::Created(id) => id,
        _ => panic!("expected created plane"),
    };
    states.push(history.scene().nodes().to_vec());
    history
        .execute(Command::SetTransform {
            id: cube,
            transform: NativeTransform {
                translation: [2.0, 3.0, 4.0],
                rotation: [45.0, 30.0, 60.0],
                scale: [-2.0, 3.0, 4.0],
            },
        })
        .unwrap();
    states.push(history.scene().nodes().to_vec());
    history
        .execute(Command::Rename {
            id: plane,
            name: "floor".into(),
        })
        .unwrap();
    states.push(history.scene().nodes().to_vec());
    history.execute(Command::Remove(cube)).unwrap();
    states.push(history.scene().nodes().to_vec());
    assert!(history.scene().node(cube).is_err());
    for expected in states[..states.len() - 1].iter().rev() {
        assert!(history.undo());
        assert_eq!(history.scene().nodes(), expected);
    }
    assert!(!history.can_undo());
    assert!(!history.undo());
    for expected in &states[1..] {
        assert!(history.redo());
        assert_eq!(history.scene().nodes(), expected);
    }
    assert!(!history.can_redo());
    assert!(!history.redo());
}

#[test]
fn failed_commands_preserve_scene_and_both_history_stacks() {
    let mut history = History::default();
    let cube = create(&mut history, "cube");
    history
        .execute(Command::Rename {
            id: cube,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    let before = history.scene().clone();
    for command in [
        Command::Create {
            primitive: Primitive::Cube,
            options: None,
            name: "".into(),
        },
        Command::Rename {
            id: cube,
            name: "\n".into(),
        },
        Command::SetTransform {
            id: cube,
            transform: NativeTransform {
                scale: [f64::NAN; 3],
                ..NativeTransform::default()
            },
        },
    ] {
        assert!(history.execute(command).is_err());
        assert_eq!(history.scene(), &before);
        assert!(history.can_undo());
        assert!(history.can_redo());
    }
    assert!(history.redo());
    assert_eq!(history.scene().node(cube).unwrap().name(), "renamed");
    assert!(history.undo());
    assert!(history.undo());
    assert!(history.scene().nodes().is_empty());
    assert!(!history.undo());
}

#[test]
fn undo_create_then_branch_reserves_discarded_ids() {
    let mut history = History::default();
    let a = create(&mut history, "a");
    let b = create(&mut history, "b");
    assert!(history.undo());
    let c = create(&mut history, "c");
    assert_ne!(b, c);
    assert_ne!(a, c);
    assert!(!history.can_redo());
    assert!(!history.redo());
    assert_eq!(
        history.scene().node(b),
        Err(NativeError::Legacy(SceneError::MissingNode(b)))
    );
    assert_eq!(
        history.execute(Command::Remove(b)),
        Err(NativeError::Legacy(SceneError::MissingNode(b)))
    );
    assert!(history.undo());
    assert!(history.undo());
    let d = create(&mut history, "d");
    for reserved in [a, b, c] {
        assert_ne!(reserved, d);
    }
}

#[test]
fn no_op_edits_do_not_consume_undo_or_invalidate_redo() {
    let mut history = History::default();
    let id = create(&mut history, "cube");
    history
        .execute(Command::Rename {
            id,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    assert_eq!(
        history
            .execute(Command::Rename {
                id,
                name: "cube".into()
            })
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(
        history
            .execute(Command::SetTransform {
                id,
                transform: NativeTransform::default()
            })
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert!(history.can_redo());
    assert!(history.redo());
    assert!(history.undo());
    assert!(history.undo());
    assert!(!history.undo());
}

#[test]
fn loaded_scene_starts_without_history_and_missing_nodes_return_errors() {
    let mut scene = NativeScene::default();
    let id = scene.create(Primitive::Cube, "existing").unwrap();
    let mut history = History::new(scene.clone());
    assert_eq!(history.scene(), &scene);
    assert!(!history.undo());
    history.execute(Command::Remove(id)).unwrap();
    let before = history.scene().clone();
    assert_eq!(
        history.execute(Command::Rename {
            id,
            name: "missing".into()
        }),
        Err(NativeError::Legacy(SceneError::MissingNode(id)))
    );
    assert_eq!(
        history.execute(Command::SetTransform {
            id,
            transform: NativeTransform::default()
        }),
        Err(NativeError::Legacy(SceneError::MissingNode(id)))
    );
    assert_eq!(history.scene(), &before);
    assert!(history.undo());
    assert_eq!(history.scene(), &scene);
    assert!(history.redo());
    assert!(history.scene().nodes().is_empty());
}

#[test]
fn zero_and_signed_extrusions_are_undoable_and_new_edits_replace_redo_without_reusing_ids() {
    let mut history = History::default();
    create(&mut history, "cube");
    let shape = history.scene().shapes()[0].id();
    let initial = history.scene().clone();
    assert_eq!(
        history
            .execute(Command::ExtrudeFace {
                shape,
                face_index: 0,
                distance: 0.0
            })
            .unwrap(),
        EditOutcome::Changed
    );
    let zero = history.scene().clone();
    history
        .execute(Command::ExtrudeFace {
            shape,
            face_index: 0,
            distance: -0.25,
        })
        .unwrap();
    let inward = history.scene().clone();
    assert!(history.undo());
    assert_eq!(history.scene(), &zero);
    assert!(history.undo());
    assert_eq!(history.scene(), &initial);
    assert!(history.redo());
    assert_eq!(history.scene(), &zero);
    assert!(history.redo());
    assert_eq!(history.scene(), &inward);
    let discarded = create(&mut history, "discarded");
    assert!(history.undo());
    history
        .execute(Command::ExtrudeFace {
            shape,
            face_index: 0,
            distance: 0.5,
        })
        .unwrap();
    assert!(!history.can_redo());
    let replacement = create(&mut history, "replacement");
    assert_ne!(replacement, discarded);
}

#[test]
fn mixed_native_draft_is_one_precise_transaction_and_failure_preserves_redo() {
    let mut history = History::default();
    let id = create(&mut history, "cube");
    let before = history.scene().clone();
    let mut channels = [None; 9];
    channels[0] = Some(16_777_216.25);
    channels[5] = Some(-0.0);
    assert_eq!(
        history
            .execute(Command::EditNode {
                id,
                name: Some("renamed".into()),
                channels,
            })
            .unwrap(),
        EditOutcome::Changed
    );
    let edited = history.scene().clone();
    let node = edited.node(id).unwrap();
    assert_eq!(node.name(), "renamed");
    assert_eq!(
        node.transform().translation[0].to_bits(),
        16_777_216.25_f64.to_bits()
    );
    assert_eq!(node.transform().rotation[2].to_bits(), (-0.0_f64).to_bits());
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
    channels[8] = Some(f64::INFINITY);
    assert!(
        history
            .execute(Command::EditNode {
                id,
                name: Some("must_not_apply".into()),
                channels,
            })
            .is_err()
    );
    assert_eq!(history.scene(), &before);
    assert!(history.can_redo());
    assert!(history.redo());
    assert_eq!(history.scene(), &edited);
}

#[test]
fn native_corner_extrusion_restores_exact_shared_mesh_snapshots() {
    let mut history = History::default();
    create(&mut history, "corner");
    let shape = history.scene().shapes()[0].id();
    let original = history.scene().clone();
    history
        .execute(Command::ExtrudeFace {
            shape,
            face_index: 1,
            distance: 0.5,
        })
        .unwrap();
    let first = history.scene().clone();
    history
        .execute(Command::ExtrudeFace {
            shape,
            face_index: 7,
            distance: 0.5,
        })
        .unwrap();
    let corner = history.scene().clone();
    assert_eq!(corner.shape(shape).unwrap().mesh().point_count(), 16);
    assert!(history.undo());
    assert_eq!(history.scene(), &first);
    assert!(history.undo());
    assert_eq!(history.scene(), &original);
    assert!(history.redo());
    assert_eq!(history.scene(), &first);
    assert!(history.redo());
    assert_eq!(history.scene(), &corner);
}

fn rename(history: &mut History, id: NodeId, n: usize) {
    history
        .execute(Command::Rename {
            id,
            name: format!("n{n}"),
        })
        .unwrap();
}

/// Mirrors the editor's dirty test (`history.scene() != saved`, src/file_ui.rs).
fn is_dirty(history: &History, saved: &NativeScene) -> bool {
    history.scene() != saved
}

#[test]
fn undo_keeps_only_the_newest_limit_entries_and_redo_is_intact() {
    let mut history = History::default();
    let id = create(&mut history, "cube");
    let mut states = vec![history.scene().nodes().to_vec()];
    for n in 0..UNDO_LIMIT + 9 {
        rename(&mut history, id, n);
        states.push(history.scene().nodes().to_vec());
    }
    let mut undone = 0;
    while history.undo() {
        undone += 1;
        let expected = &states[states.len() - 1 - undone];
        assert_eq!(history.scene().nodes(), expected);
    }
    assert_eq!(undone, UNDO_LIMIT);
    assert!(!history.can_undo());
    assert_ne!(history.scene().nodes(), states[0].as_slice());
    let mut redone = 0;
    while history.redo() {
        redone += 1;
    }
    assert_eq!(redone, UNDO_LIMIT);
    assert_eq!(history.scene().nodes(), states.last().unwrap().as_slice());
}

#[test]
fn new_edit_at_the_limit_clears_redo_and_still_drops_only_the_oldest() {
    let mut history = History::default();
    let id = create(&mut history, "cube");
    for n in 0..UNDO_LIMIT {
        rename(&mut history, id, n);
    }
    assert!(history.undo());
    assert!(history.can_redo());
    rename(&mut history, id, 1000);
    assert!(!history.can_redo());
    // The stack is full again; this edit must drop the oldest entry ("cube").
    rename(&mut history, id, 1001);
    let mut undone = 0;
    while history.undo() {
        undone += 1;
    }
    assert_eq!(undone, UNDO_LIMIT);
    assert_eq!(history.scene().node(id).unwrap().name(), "n0");
}

#[test]
fn scene_stays_dirty_after_the_saved_entry_is_dropped() {
    let mut history = History::default();
    let id = create(&mut history, "cube");
    let saved = history.scene().clone();
    assert!(!is_dirty(&history, &saved));
    for n in 0..UNDO_LIMIT + 5 {
        rename(&mut history, id, n);
    }
    assert!(is_dirty(&history, &saved));
    while history.undo() {
        assert!(
            is_dirty(&history, &saved),
            "oldest kept entry must not read as the saved scene"
        );
    }
    assert!(!history.can_undo() && is_dirty(&history, &saved));
}

#[test]
fn dirty_flag_clears_when_undoing_back_to_a_retained_saved_state() {
    let mut history = History::default();
    let id = create(&mut history, "cube");
    rename(&mut history, id, 0);
    let saved = history.scene().clone();
    for n in 1..=UNDO_LIMIT {
        rename(&mut history, id, n);
    }
    for _ in 0..UNDO_LIMIT {
        assert!(history.undo());
    }
    assert!(!is_dirty(&history, &saved));
}

mod primitive_options {
    use azteccraft::history::{Command, EditOutcome, History};
    use azteccraft::primitive_options::{Axis, PrimitiveOptions};
    use azteccraft::scene::{Primitive, SceneError};

    fn sphere(radius: f64) -> PrimitiveOptions {
        PrimitiveOptions::Sphere {
            radius,
            sub_axis: 8,
            sub_height: 6,
            axis: Axis::X,
        }
    }

    fn create(
        history: &mut History,
        primitive: Primitive,
        options: Option<PrimitiveOptions>,
    ) -> Result<EditOutcome, azteccraft::native_scene::NativeError> {
        history.execute(Command::Create {
            primitive,
            options,
            name: "made".into(),
        })
    }

    #[test]
    fn rejected_options_leave_scene_and_history_untouched() {
        let mut history = History::default();
        create(&mut history, Primitive::Cube, None).unwrap();
        let scene = history.scene().clone();
        let undo = history.undo_label();
        let invalid = [
            (Primitive::Sphere, sphere(-1.0)),
            (Primitive::Sphere, sphere(f64::NAN)),
            (Primitive::Sphere, sphere(1e6 + 1.0)),
            (Primitive::Cone, sphere(1.0)),
            (
                Primitive::Cube,
                PrimitiveOptions::default_for(Primitive::Plane),
            ),
        ];
        for (primitive, options) in invalid {
            let error = create(&mut history, primitive, Some(options)).unwrap_err();
            assert!(
                matches!(
                    error,
                    azteccraft::native_scene::NativeError::Legacy(
                        SceneError::InvalidPrimitiveOptions(_)
                    )
                ),
                "{primitive:?}: {error:?}"
            );
            assert_eq!(history.scene(), &scene);
            assert_eq!(history.undo_label(), undo);
            assert!(!history.can_redo());
        }
        assert!(history.undo());
        assert!(!history.can_undo());
    }

    #[test]
    fn optioned_create_undoes_and_redoes_exactly() {
        let mut history = History::default();
        let before = history.scene().clone();
        create(&mut history, Primitive::Sphere, Some(sphere(2.5))).unwrap();
        let after = history.scene().clone();
        assert_ne!(after, before);
        assert!(history.undo());
        assert_eq!(history.scene(), &before);
        assert!(history.redo());
        assert_eq!(history.scene(), &after);
    }

    #[test]
    fn none_and_explicit_defaults_build_identical_scenes() {
        for primitive in [
            Primitive::Cube,
            Primitive::Plane,
            Primitive::Sphere,
            Primitive::Cylinder,
            Primitive::Cone,
        ] {
            let mut implicit = History::default();
            let mut explicit = History::default();
            create(&mut implicit, primitive, None).unwrap();
            create(
                &mut explicit,
                primitive,
                Some(PrimitiveOptions::default_for(primitive)),
            )
            .unwrap();
            let a = implicit.scene().shapes()[0].mesh();
            let b = explicit.scene().shapes()[0].mesh();
            assert_eq!(a, b, "{primitive:?}");
        }
    }
}
