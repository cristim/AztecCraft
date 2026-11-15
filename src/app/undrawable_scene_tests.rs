//! A scene that cannot be drawn because a node is scaled past f32 range. The edit tools refuse
//! that scale (#396), so a test reaches it through Undo.
use azteccraft::native_scene::NativeTransform;
use azteccraft::scene::NodeId;

/// A history whose current scene is `scene` and whose one Undo step leads to the same scene with
/// node `id` scaled out of f32 range, so a view build fails after the Undo and Redo repairs it.
/// The edit tools refuse that scale, so a test reaches the undrawable scene this way.
pub(crate) fn history_that_undoes_into_an_undrawable_scale(
    scene: &azteccraft::native_scene::NativeScene,
    id: NodeId,
) -> azteccraft::history::History {
    let transform = scene.node(id).unwrap().transform();
    let mut undrawable = scene.clone();
    undrawable
        .set_transform(
            id,
            NativeTransform {
                scale: [1e300; 3],
                ..transform
            },
        )
        .unwrap();
    let mut history = azteccraft::history::History::new(undrawable);
    history
        .execute(azteccraft::history::Command::SetTransform { id, transform })
        .unwrap();
    history
}
