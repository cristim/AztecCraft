//! Lead and member colours for a multi-object selection, through the CPU submission. Colours are
//! our own and provisional (development.md row 33); this proves they differ, not Maya parity.

use super::*;
use azteccraft::native_scene::TransformTarget;

fn three_cubes() -> (NativeScene, Vec<TransformTarget>) {
    let mut scene = NativeScene::default();
    for name in ["a", "b", "c"] {
        scene.create(Primitive::Cube, name).unwrap();
    }
    let targets = scene.transform_paths().unwrap();
    assert_eq!(targets.len(), 3);
    (scene, targets)
}

fn submission(scene: &NativeScene, feedback: SelectionFeedback<'_>) -> Submission {
    let view = ViewScene::from_native(scene, VIEW_BUDGET).unwrap();
    Submission::new(
        &view,
        &camera(0.01),
        [SIZE; 2],
        feedback,
        true,
        ShadingMode::SmoothShadeAll,
        FORMAT,
        u64::MAX,
    )
    .unwrap()
}

fn count(colors: &[[f32; 4]], rgb: [u8; 3]) -> usize {
    let wanted = Color32::from_rgb(rgb[0], rgb[1], rgb[2]).to_normalized_gamma_f32();
    colors.iter().filter(|color| **color == wanted).count()
}

fn line_colors(submission: &Submission) -> Vec<[f32; 4]> {
    submission
        .lines
        .clone()
        .map(|vertex| {
            let start = vertex as usize * VERTEX_BYTES + 4 * size_of::<f32>();
            std::array::from_fn(|channel| {
                let at = start + channel * size_of::<f32>();
                f32::from_ne_bytes(submission.bytes[at..at + 4].try_into().unwrap())
            })
        })
        .collect()
}

#[test]
fn members_are_tinted_the_lead_differs_and_unselected_is_untouched() {
    let (scene, targets) = three_cubes();
    let picked = [targets[0].clone(), targets[1].clone()];
    let colors = surface_colors(&submission(&scene, SelectionFeedback::Multi(&picked)));
    // Each cube's +Z face faces the eye, so six vertices carry its unlit base.
    assert_eq!(count(&colors, [95, 110, 120]), 6, "one member tint");
    assert_eq!(count(&colors, [100, 125, 100]), 6, "one lead tint");
    assert_eq!(count(&colors, [83, 83, 83]), 6, "one untouched cube");
}

#[test]
fn the_lead_is_the_last_member_not_the_first() {
    let (scene, targets) = three_cubes();
    let forward = [targets[0].clone(), targets[1].clone()];
    let reverse = [targets[1].clone(), targets[0].clone()];
    let first = surface_colors(&submission(&scene, SelectionFeedback::Multi(&forward)));
    let second = surface_colors(&submission(&scene, SelectionFeedback::Multi(&reverse)));
    assert_ne!(first, second);
}

#[test]
fn single_selection_pixels_are_unchanged() {
    let (scene, targets) = three_cubes();
    let path = &targets[1].path;
    let colors = surface_colors(&submission(&scene, SelectionFeedback::Object(Some(path))));
    assert_eq!(count(&colors, [95, 110, 120]), 6);
    assert_eq!(count(&colors, [100, 125, 100]), 0);
    assert_eq!(count(&colors, [83, 83, 83]), 12);
}

#[test]
fn member_and_lead_edges_use_their_own_colours_and_unselected_stay_grey() {
    let (scene, targets) = three_cubes();
    let picked = [targets[0].clone(), targets[1].clone()];
    let lines = line_colors(&submission(&scene, SelectionFeedback::Multi(&picked)));
    assert!(count(&lines, [242, 163, 70]) > 0, "member edges");
    assert!(count(&lines, [140, 240, 140]) > 0, "lead edges");
    assert!(lines.contains(&Color32::from_gray(145).to_normalized_gamma_f32()));
}

#[test]
fn a_curve_member_and_a_curve_lead_are_highlighted() {
    let scene = curve_scene(true, false, 0.0);
    let targets = scene.transform_paths().unwrap();
    assert_eq!(targets.len(), 2);
    let lines = line_colors(&submission(&scene, SelectionFeedback::Multi(&targets)));
    assert!(count(&lines, [242, 163, 70]) > 0, "first curve is a member");
    assert!(
        count(&lines, [140, 240, 140]) > 0,
        "second curve is the lead"
    );
    let one = [targets[0].clone()];
    let lines = line_colors(&submission(&scene, SelectionFeedback::Multi(&one)));
    assert_eq!(count(&lines, [242, 163, 70]), 0, "a lone Multi leads");
    assert!(count(&lines, [140, 240, 140]) > 0);
}

#[test]
fn selection_colours_are_pinned() {
    assert_eq!(SELECTION_TINT_RGB, [107, 137, 157]);
    assert_eq!(LEAD_TINT_RGB, [117, 167, 117]);
    assert_eq!(SELECTED_EDGE_RGB_OBJECT, [242, 163, 70]);
    assert_eq!(LEAD_EDGE_RGB, [140, 240, 140]);
    for base in [[83; 3], [255; 3], [0; 3], [255, 0, 0], LEAD_TINT_RGB] {
        assert_ne!(lead_base(base), base, "{base:?}");
        assert_ne!(lead_base(base), selected_base(base), "{base:?}");
    }
}

#[test]
fn the_lead_edges_are_submitted_after_the_member_edges() {
    let (scene, targets) = three_cubes();
    let picked = [targets[0].clone(), targets[1].clone()];
    let lines = line_colors(&submission(&scene, SelectionFeedback::Multi(&picked)));
    let position = |rgb: [u8; 3]| -> Vec<usize> {
        let wanted = Color32::from_rgb(rgb[0], rgb[1], rgb[2]).to_normalized_gamma_f32();
        (0..lines.len()).filter(|i| lines[*i] == wanted).collect()
    };
    let members = position([242, 163, 70]);
    let leads = position([140, 240, 140]);
    assert!(!members.is_empty() && !leads.is_empty());
    assert!(
        members.iter().max() < leads.iter().min(),
        "lead is drawn last"
    );
}

#[test]
fn descendants_take_the_colour_of_the_selected_ancestor_they_sit_below() {
    // PROVISIONAL (known-issues.md): matching is by path prefix, so a descendant of the lead is
    // drawn as the lead and a descendant of a plain member as a member; Maya is unconfirmed.
    let native = nested_and_flat_scene();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let roots = native.transform_paths().unwrap();
    let root = roots.iter().find(|t| t.path.len() == 1).unwrap().clone();
    let other = roots.iter().rfind(|t| t.path.len() == 1).unwrap().clone();
    let nested = view
        .instances()
        .iter()
        .position(|i| i.path().len() > 2)
        .unwrap();
    let flat = 1 - nested;
    let highlight = |members: &[TransformTarget], index: usize| {
        let instance = &view.instances()[index];
        SelectionFeedback::Multi(members).highlight(instance, &instance.faces()[0])
    };
    let root_leads = [other.clone(), root.clone()];
    assert!(highlight(&root_leads, nested) == Highlight::Lead);
    assert!(highlight(&root_leads, flat) == Highlight::Member);
    let other_leads = [root, other];
    assert!(highlight(&other_leads, nested) == Highlight::Member);
    assert!(highlight(&other_leads, flat) == Highlight::Lead);
}
