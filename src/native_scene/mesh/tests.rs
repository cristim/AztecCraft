use super::*;

#[test]
fn defensive_private_mesh_states_refuse_the_actual_authored_prefix() {
    use crate::native_scene::ascii::{AsciiWriteRefusal as R, emit_geometry_prefix as emit};
    type Case = (fn(&mut NativeMesh), R);
    let cases: [Case; 4] = [
        (
            |m| {
                m.point_offsets.push(PointOffset {
                    vertex: 0,
                    delta_cm: [0.0; 3],
                })
            },
            R::PointOffsets,
        ),
        (|m| m.edges[0].smooth = Some(true), R::EdgeSmoothing),
        (|m| m.edges[0].smooth = None, R::EdgeSmoothing),
        (
            |m| m.unsupported_channels.push("unknown".into()),
            R::UnsupportedMeshChannels,
        ),
    ];
    for (mutate, expected) in cases {
        let (mut scene, _) = crate::native_scene::test_fixtures::cube();
        scene.validate().unwrap();
        let baseline = scene.clone();
        emit(&scene).unwrap();
        assert_eq!(scene, baseline);
        mutate(&mut scene.shapes[0].mesh);
        scene.validate().unwrap();
        let before = scene.clone();
        assert_eq!(emit(&scene), Err(expected));
        assert_eq!(scene, before);
        assert_eq!(
            (scene.next_id, scene.next_native_id),
            (before.next_id, before.next_native_id)
        );
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let (mut scene, _) = crate::native_scene::test_fixtures::cube();
        scene.validate().unwrap();
        emit(&scene).unwrap();
        let Points::Float3(points) = &mut scene.shapes[0].mesh.points else {
            panic!()
        };
        points[0][0] = value;
        let before = scene.clone();
        assert_eq!(emit(&scene), Err(R::InvalidScene(NativeError::NonFinite)));
        assert_eq!(scene, before);
        assert_eq!(
            (scene.next_id, scene.next_native_id),
            (before.next_id, before.next_native_id)
        );
    }
}

#[test]
fn authored_faces_keep_admitted_repeated_loop_and_unused_edges() {
    use crate::native_scene::ascii::emit_geometry_prefix;
    let (mut scene, _) = crate::native_scene::test_fixtures::cube();
    let mut mesh = NativeMesh::with_options(&crate::primitive_options::SINGLE_QUAD_PLANE).unwrap();
    let repeated = mesh.faces[0].outer.clone();
    mesh.faces[0].outer.extend(repeated);
    mesh.add_loose_edge_fixture(mesh.edges[0].endpoints());
    mesh.add_loose_edge_fixture([0, 0]);
    scene.shapes[0].mesh = mesh;
    scene.validate().unwrap();
    let before = scene.clone();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    assert_eq!(scene, before);
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("\tsetAttr -s 6 \".ed[0:5]\"\n"));
    assert!(text.contains("\t0 1 0\n\t0 0 0\n;\n"));
    assert!(
        bytes.ends_with(
            b"\tsetAttr -s 1 \".fc[0:0]\" -type \"polyFaces\"\n\tf 8 0 1 2 3 0 1 2 3\n;\n"
        )
    );
}

#[test]
fn malformed_stored_face_refuses_before_any_output() {
    use crate::native_scene::ascii::{AsciiWriteRefusal, emit_geometry_prefix};
    let (mut scene, _) = crate::native_scene::test_fixtures::cube();
    scene.shapes[0].mesh.faces[0].outer[0].edge = usize::MAX;
    let before = scene.clone();
    assert_eq!(
        emit_geometry_prefix(&scene),
        Err(AsciiWriteRefusal::InvalidScene(
            NativeError::InvalidTopology
        ))
    );
    assert_eq!(scene, before);
}

#[test]
fn binary_sparse_offsets_preserve_storage_and_add_before_world_narrowing() {
    let mut mesh = NativeMesh::offset_triangle_fixture();
    let original = mesh.clone();
    assert_eq!(mesh.point_count(), 3);
    assert_eq!(mesh.effective_point_cm(0).unwrap()[0] - 16777216.0, 0.25);
    assert_eq!(mesh.effective_point_cm(1).unwrap(), [16777218.0, 0.0, 0.0]);
    assert_eq!(mesh.edges[0].smooth(), None);
    mesh.point_offsets.push(PointOffset {
        vertex: 2,
        delta_cm: [-0.5, 0.0, -0.0],
    });
    mesh.validate().unwrap();
    assert_eq!(mesh.effective_point_cm(2).unwrap(), [16777215.5, 1.0, 0.0]);
    assert_eq!(mesh.points, original.points);
    assert_eq!(mesh.clone(), mesh);
    assert_ne!(
        mesh.point_offsets[0].delta_cm[1].to_bits(),
        0.0f32.to_bits()
    );
    let mut positive_zero = mesh.clone();
    positive_zero.point_offsets[0].delta_cm[1] = 0.0;
    assert_ne!(mesh, positive_zero);
    assert_eq!(
        mesh.extruded_face(0, 1.0),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(
        mesh.effective_point_cm(3),
        Err(NativeError::InvalidTopology)
    );
    assert_eq!(
        NativeMesh::primitive(Primitive::Cube).unwrap().edges[0].smooth(),
        Some(false)
    );
    let summary = format!("{mesh:?} {:?}", mesh.points);
    assert!(!summary.contains("16777216"));
    assert!(summary.len() < 256);
    assert!(format!("{:?}", mesh.faces[0]).len() < 128);
}

#[test]
fn binary_factory_rejects_invalid_offsets_topology_and_guessed_smoothing() {
    let baseline = NativeMesh::offset_triangle_fixture();
    for offsets in [
        vec![PointOffset {
            vertex: 3,
            delta_cm: [0.0; 3],
        }],
        vec![PointOffset {
            vertex: 0,
            delta_cm: [f32::NAN; 3],
        }],
        vec![PointOffset {
            vertex: 0,
            delta_cm: [f32::INFINITY; 3],
        }],
        vec![
            PointOffset {
                vertex: 0,
                delta_cm: [0.0; 3]
            };
            2
        ],
        vec![
            PointOffset {
                vertex: 2,
                delta_cm: [0.0; 3],
            },
            PointOffset {
                vertex: 0,
                delta_cm: [0.0; 3],
            },
        ],
    ] {
        let Points::Float3(base) = baseline.points.clone() else {
            panic!()
        };
        assert!(
            NativeMesh::from_binary_parts(
                base,
                baseline.edges.clone(),
                baseline.faces.clone(),
                offsets
            )
            .is_err()
        );
    }
    let Points::Float3(base) = baseline.points.clone() else {
        panic!()
    };
    let mut edges = baseline.edges.clone();
    edges[0].smooth = Some(false);
    assert_eq!(
        NativeMesh::from_binary_parts(base.clone(), edges, baseline.faces.clone(), Vec::new()),
        Err(NativeError::UnsupportedEdit)
    );
    let mut faces = baseline.faces.clone();
    faces[0].outer[0].reversed = true;
    assert_eq!(
        NativeMesh::from_binary_parts(base.clone(), baseline.edges.clone(), faces, Vec::new()),
        Err(NativeError::InvalidTopology)
    );
    let mut extra = base;
    extra.push([99.0, -0.0, 1.0]);
    let mesh = NativeMesh::from_binary_parts(
        extra.clone(),
        baseline.edges.clone(),
        baseline.faces.clone(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(mesh.points, Points::Float3(extra));
    assert_eq!(mesh.faces, baseline.faces);
    assert_eq!(mesh.edges, baseline.edges);
}

#[test]
fn each_unproved_topology_update_refuses_independently() {
    let mut unknown = NativeMesh::primitive(Primitive::Cube).unwrap();
    unknown.edges[0].smooth = None;
    assert!(unknown.point_offsets.is_empty());
    assert_eq!(
        unknown.extruded_face(1, 1.0),
        Err(NativeError::UnsupportedEdit)
    );
    let mut offsets = NativeMesh::primitive(Primitive::Cube).unwrap();
    offsets.point_offsets.push(PointOffset {
        vertex: 0,
        delta_cm: [0.0; 3],
    });
    assert!(offsets.edges.iter().all(|e| e.smooth.is_some()));
    assert_eq!(
        offsets.extruded_face(1, 1.0),
        Err(NativeError::UnsupportedEdit)
    );
}

#[test]
fn double_and_float_extrusion_preserve_original_bits_and_edge_metadata() {
    for doubles in [false, true] {
        for distance in [0.0, 0.123456789012] {
            let mut mesh = NativeMesh::primitive(Primitive::Cube).unwrap();
            if doubles {
                let mut points = mesh
                    .points
                    .values()
                    .into_iter()
                    .map(|p| p.to_array())
                    .collect::<Vec<_>>();
                points[0][0] += 1e-10;
                points[0][1] = -0.0;
                points[4][0] -= 1e-10;
                mesh.points = Points::Double3(points);
            } else if let Points::Float3(points) = &mut mesh.points {
                points[0][1] = -0.0;
            }
            mesh.edges[0].smooth = Some(true);
            mesh.edges[5].smooth = Some(true);
            let before = mesh.clone();
            let after = mesh.extruded_face(1, distance).unwrap();
            let old_len = before.points.values().len();
            let prefix = match &after.points {
                Points::Float3(v) => Points::Float3(v[..old_len].into()),
                Points::Double3(v) => Points::Double3(v[..old_len].into()),
            };
            assert_eq!(prefix, before.points);
            assert_eq!(&after.edges[..before.edges.len()], before.edges.as_slice());
            assert_eq!(&after.faces[2..6], &before.faces[2..6]);
            assert_eq!(after.faces[0], before.faces[0]);
            assert_eq!(after.edges.len(), before.edges.len() + 8);
            for i in 0..4 {
                assert_eq!(after.faces[6 + i].outer[0], before.faces[1].outer[i]);
            }
            assert_eq!(matches!(after.points, Points::Double3(_)), doubles);
            if let Points::Double3(points) = &after.points {
                assert_eq!(points[8][0].to_bits(), (-0.5f64 - 1e-10).to_bits());
                assert_eq!(points[8][2].to_bits(), (0.5 + distance).to_bits());
            }
            after.validate().unwrap();
        }
    }
}

#[test]
fn arbitrary_loops_holes_and_reversed_zero_edge_are_retained_without_projection() {
    let mut mesh = NativeMesh::primitive(Primitive::Cube).unwrap();
    let original_face = mesh.faces[0].clone();
    mesh.faces[0].holes = vec![original_face.outer.clone()];
    mesh.validate().unwrap();
    assert_eq!(mesh.faces[0].holes[0][0].edge, 0);
    let before = mesh.clone();
    assert!(mesh.view(1.0).is_err());
    assert_eq!(
        mesh.extruded_face(0, 1.0),
        Err(NativeError::UnsupportedEdit)
    );
    let after = mesh.extruded_face(1, 1.0).unwrap();
    assert_eq!(after.faces[0], before.faces[0]);
    let zero = DirectedEdge {
        edge: 0,
        reversed: true,
    };
    assert_eq!(
        mesh.endpoints(zero).unwrap(),
        [mesh.edges[0].endpoints[1], mesh.edges[0].endpoints[0]]
    );
    let points = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.5, 2.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let mut ngon = NativeMesh {
        point_offsets: Vec::new(),
        points: Points::Double3(points),
        edges: Vec::new(),
        faces: Vec::new(),
        unsupported_channels: Vec::new(),
    };
    let outer = (0..5).map(|i| ngon.append_edge([i, (i + 1) % 5])).collect();
    ngon.faces.push(NativeFace {
        outer,
        holes: Vec::new(),
    });
    ngon.validate().unwrap();
    assert_eq!(
        ngon.view(1.0),
        Err(NativeError::UnsupportedView("non-quad polygon"))
    );
}

#[test]
fn unsupported_channel_updates_and_new_float_overflow_refuse_without_mutation() {
    for channel in ["crease", "UV", "normal", "color", "tweak", "dependency"] {
        let mut mesh = NativeMesh::primitive(Primitive::Cube).unwrap();
        mesh.unsupported_channels.push(channel.into());
        let before = mesh.clone();
        assert_eq!(
            mesh.extruded_face(1, 1.0),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(mesh, before);
    }
    let mesh = NativeMesh::primitive(Primitive::Cube).unwrap();
    assert_eq!(
        mesh.extruded_face(1, f64::from(f32::MAX) * 2.0),
        Err(NativeError::NonFinite)
    );
    let mut mesh = mesh;
    mesh.points = Points::Double3(
        mesh.points
            .values()
            .into_iter()
            .map(|p| p.to_array())
            .collect(),
    );
    let double = mesh.extruded_face(1, f64::from(f32::MAX) * 2.0).unwrap();
    double.validate().unwrap();
    assert!(double.view(1.0).is_err());
    for value in [f64::NAN, f64::INFINITY] {
        if let Points::Double3(points) = &mut mesh.points {
            points[0][0] = value;
        }
        assert_eq!(mesh.validate(), Err(NativeError::NonFinite));
    }
}
