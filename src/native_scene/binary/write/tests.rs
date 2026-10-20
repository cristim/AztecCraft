use super::*;
use crate::scene::Primitive;

fn bare(names: &[&str]) -> NativeScene {
    let mut scene = NativeScene::default();
    for name in names {
        scene.create(Primitive::Cube, name).unwrap();
    }
    scene.shapes.clear();
    scene.dag.retain(|edge| edge.parent.is_none());
    scene
}

#[test]
fn nothing_is_returned_while_emission_is_incomplete() {
    let mut cube = NativeScene::default();
    cube.create(Primitive::Cube, "cube").unwrap();
    for scene in [cube, bare(&["a", "b"]), NativeScene::default()] {
        assert_eq!(
            encode_authored(&scene),
            Err(AuthoredBinaryRefusal::NotYetImplemented)
        );
    }
}

#[test]
fn eligibility_refusals_come_before_the_incomplete_refusal() {
    let mut cube = NativeScene::default();
    cube.create(Primitive::Cube, "persp").unwrap();
    for scene in [cube, bare(&["persp"])] {
        assert_eq!(
            encode_authored(&scene),
            Err(AuthoredBinaryRefusal::Ineligible(
                AuthoredIneligible::ReservedName
            ))
        );
    }
}
