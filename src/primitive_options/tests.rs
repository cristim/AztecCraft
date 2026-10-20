use super::*;

const ALL: [Primitive; 5] = [
    Primitive::Cube,
    Primitive::Plane,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Cone,
];

fn sphere(radius: f64, sub_axis: u32, sub_height: u32) -> PrimitiveOptions {
    PrimitiveOptions::Sphere {
        radius,
        sub_axis,
        sub_height,
        axis: Axis::Y,
    }
}

#[test]
fn defaults_match_the_documented_table() {
    for primitive in ALL {
        let options = PrimitiveOptions::default_for(primitive);
        assert_eq!(options.primitive(), primitive);
    }
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Sphere),
        sphere(1.0, 20, 20)
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cone),
        PrimitiveOptions::Cone {
            radius: 1.0,
            height: 2.0,
            sub_axis: 20,
            sub_height: 1,
            axis: Axis::Y
        }
    );
    assert_eq!(Axis::default(), Axis::Y);
}

#[test]
fn face_counts_cover_caps_and_saturate() {
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cube).face_count(),
        6
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Plane).face_count(),
        1
    );
    assert_eq!(sphere(1.0, 8, 6).face_count(), 48);
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cylinder).face_count(),
        22
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cone).face_count(),
        21
    );
    let huge = PrimitiveOptions::Cube {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
        sub_width: u32::MAX,
        sub_height: u32::MAX,
        sub_depth: u32::MAX,
        axis: Axis::Y,
    };
    assert_eq!(huge.face_count(), u64::MAX);
}
