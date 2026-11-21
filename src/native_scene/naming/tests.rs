//! Prefix Hierarchy Names and Search and Replace Names on the native scene.
use super::*;
use crate::native_scene::group::tests::cube;
use crate::native_scene::{ReparentKeep, test_fixtures};

fn names(scene: &NativeScene) -> Vec<String> {
    scene.nodes.iter().map(|n| n.name.clone()).collect()
}

fn shape_names(scene: &NativeScene) -> Vec<String> {
    scene.shapes.iter().map(|s| s.name.clone()).collect()
}

/// root > mid > leaf, plus an unrelated `other`. Returns (scene, root, mid, leaf, other).
fn family() -> (NativeScene, NodeId, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let [root, mid, leaf, other] = ["root", "mid", "leaf", "other"].map(|n| cube(&mut scene, n));
    scene
        .reparent(mid, Some(root), ReparentKeep::Local)
        .unwrap();
    scene
        .reparent(leaf, Some(mid), ReparentKeep::Local)
        .unwrap();
    (scene, root, mid, leaf, other)
}

fn name(scene: &NativeScene, id: NodeId) -> String {
    scene.node(id).unwrap().name.clone()
}

#[test]
fn prefix_reaches_the_selection_and_every_descendant_exactly_once() {
    let (mut scene, root, mid, leaf, other) = family();
    scene.prefix_hierarchy_names(&[root], "a_").unwrap();
    assert_eq!(name(&scene, root), "a_root");
    assert_eq!(name(&scene, mid), "a_mid");
    assert_eq!(name(&scene, leaf), "a_leaf");
    assert_eq!(name(&scene, other), "other");
}

#[test]
fn prefix_with_selected_ancestor_and_descendant_prefixes_each_once() {
    let (mut scene, root, _, leaf, _) = family();
    scene
        .prefix_hierarchy_names(&[leaf, root, root], "p")
        .unwrap();
    assert_eq!(names(&scene), ["proot", "pmid", "pleaf", "other"]);
}

#[test]
fn prefix_follows_the_derived_shape_names() {
    let (mut scene, root, ..) = family();
    scene.prefix_hierarchy_names(&[root], "a_").unwrap();
    assert!(shape_names(&scene).iter().any(|n| n == "a_rootShape"));
}

#[test]
fn an_instanced_node_is_renamed_once() {
    let (mut scene, x1, x2, a, b) = test_fixtures::instanced_chain();
    scene.prefix_hierarchy_names(&[x1, x2], "z").unwrap();
    assert_eq!(name(&scene, a), "za");
    assert_eq!(name(&scene, b), "zb");
    assert_eq!(name(&scene, x1), "zx1");
}

#[test]
fn prefix_refusals_change_nothing() {
    let (mut scene, root, ..) = family();
    let before = scene.clone();
    let refused = |scene: &mut NativeScene, ids: &[NodeId], prefix: &str| {
        let error = scene.prefix_hierarchy_names(ids, prefix).unwrap_err();
        assert_eq!(*scene, before);
        error
    };
    assert_eq!(
        refused(&mut scene, &[root], ""),
        NamingRefusal::EmptyPrefix.into()
    );
    assert_eq!(
        refused(&mut scene, &[], "a"),
        NamingRefusal::EmptySet.into()
    );
    assert_eq!(refused(&mut scene, &[root], "1"), NativeError::InvalidName);
    assert_eq!(
        refused(&mut scene, &[root], "a b"),
        NativeError::InvalidName
    );
    assert_eq!(refused(&mut scene, &[root], "é"), NativeError::InvalidName);
}

#[test]
fn a_prefix_that_makes_two_names_equal_is_a_collision_and_changes_nothing() {
    let (mut scene, root, mid, ..) = family();
    cube(&mut scene, "x_mid");
    scene.rename(root, "a").unwrap();
    let before = scene.clone();
    // "x_" + "mid" collides with the unrelated "x_mid", found at the second node.
    assert_eq!(
        scene.prefix_hierarchy_names(&[root], "x_"),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);
    assert_eq!(name(&scene, mid), "mid");
}

#[test]
fn a_collision_on_the_third_node_leaves_the_first_two_untouched() {
    let (mut scene, root, mid, leaf, _) = family();
    cube(&mut scene, "q_leaf");
    let before = scene.clone();
    assert_eq!(
        scene.prefix_hierarchy_names(&[root], "q_"),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);
    assert_eq!(name(&scene, root), "root");
    assert_eq!(name(&scene, mid), "mid");
    assert_eq!(name(&scene, leaf), "leaf");
}

#[test]
fn a_missing_node_is_refused_and_changes_nothing() {
    let (mut scene, ..) = family();
    let before = scene.clone();
    let missing = NodeId::from_decimal("999999").unwrap();
    assert!(scene.prefix_hierarchy_names(&[missing], "a").is_err());
    assert!(
        scene
            .search_replace_names(&[missing], true, "o", "x", true)
            .is_err()
    );
    assert_eq!(scene, before);
}

#[test]
fn replace_edits_only_the_selected_nodes_without_hierarchy() {
    let (mut scene, root, mid, ..) = family();
    scene
        .search_replace_names(&[root], false, "o", "0", true)
        .unwrap();
    assert_eq!(name(&scene, root), "r00t");
    assert_eq!(name(&scene, mid), "mid");
}

#[test]
fn replace_with_hierarchy_reaches_descendants() {
    let (mut scene, root, mid, leaf, other) = family();
    scene
        .search_replace_names(&[root], true, "e", "E", true)
        .unwrap();
    assert_eq!(name(&scene, root), "root");
    assert_eq!(name(&scene, mid), "mid");
    assert_eq!(name(&scene, leaf), "lEaf");
    assert_eq!(name(&scene, other), "other");
}

#[test]
fn replace_all_occurrences_without_overlap() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "aaaa");
    scene
        .search_replace_names(&[id], false, "aa", "b", true)
        .unwrap();
    assert_eq!(name(&scene, id), "bb");
}

#[test]
fn replace_case_sensitivity() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "FooBAR");
    assert_eq!(
        scene.search_replace_names(&[id], false, "bar", "x", true),
        Err(NamingRefusal::NothingToRename.into())
    );
    scene
        .search_replace_names(&[id], false, "bar", "x", false)
        .unwrap();
    assert_eq!(name(&scene, id), "Foox");
}

#[test]
fn replace_refusals_change_nothing() {
    let (mut scene, root, ..) = family();
    let before = scene.clone();
    let cases: [(&str, &str, NativeError); 6] = [
        ("", "x", NamingRefusal::EmptySearch.into()),
        ("zzz", "x", NamingRefusal::NothingToRename.into()),
        ("root", "", NativeError::InvalidName),
        ("root", "1", NativeError::InvalidName),
        ("root", "é", NativeError::InvalidName),
        ("root", "other", NativeError::NameCollision),
    ];
    for (search, replace, want) in cases {
        let got = scene.search_replace_names(&[root], true, search, replace, true);
        assert_eq!(got, Err(want), "{search:?} -> {replace:?}");
        assert_eq!(scene, before);
    }
}

#[test]
fn a_unicode_search_matches_nothing_and_replacing_to_the_same_name_is_nothing_to_rename() {
    let (mut scene, root, ..) = family();
    assert_eq!(
        scene.search_replace_names(&[root], true, "é", "e", true),
        Err(NamingRefusal::NothingToRename.into())
    );
    assert_eq!(
        scene.search_replace_names(&[root], true, "root", "root", true),
        Err(NamingRefusal::NothingToRename.into())
    );
}

#[test]
fn replace_is_atomic_when_the_third_match_clashes() {
    let (mut scene, root, mid, leaf, _) = family();
    scene.rename(root, "l_root").unwrap();
    scene.rename(mid, "l_mid").unwrap();
    scene.rename(leaf, "l_leaf").unwrap();
    cube(&mut scene, "x_leaf");
    let before = scene.clone();
    // l_root and l_mid become x_root and x_mid; l_leaf -> x_leaf clashes at the third node.
    assert_eq!(
        scene.search_replace_names(&[root], true, "l_", "x_", true),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);
    assert_eq!(name(&scene, root), "l_root");
}

#[test]
fn swapping_two_names_through_replace_is_a_collision_not_a_silent_merge() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "ab");
    let b = cube(&mut scene, "ba");
    // "a"->"b" turns ab into bb and ba into bb: two equal new names.
    assert_eq!(
        scene.search_replace_names(&[a, b], false, "a", "b", true),
        Err(NativeError::NameCollision)
    );
}

#[test]
fn a_node_may_take_the_name_another_renamed_node_gives_up() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a1");
    let b = cube(&mut scene, "a2");
    scene
        .search_replace_names(&[a, b], false, "a", "c", true)
        .unwrap();
    assert_eq!(names(&scene), ["c1", "c2"]);
}
