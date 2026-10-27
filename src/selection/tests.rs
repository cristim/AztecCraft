use super::*;
use crate::native_scene::NativeId;
use crate::scene::NodeId;

fn node(value: u64) -> NodeId {
    NodeId::from_decimal(&value.to_string()).unwrap()
}

fn target(value: u64, path: &[u64]) -> TransformTarget {
    TransformTarget {
        node: node(value),
        path: path
            .iter()
            .map(|id| NativeId::from_decimal(&id.to_string()).unwrap())
            .collect(),
    }
}

fn plain(value: u64) -> TransformTarget {
    target(value, &[value])
}

fn selection_of(values: &[u64]) -> ObjectSelection {
    let mut selection = ObjectSelection::default();
    for value in values {
        selection.add(plain(*value)).unwrap();
    }
    selection
}

fn nodes(selection: &ObjectSelection) -> Vec<NodeId> {
    selection.members().iter().map(|t| t.node).collect()
}

#[test]
fn empty_selection_has_no_lead() {
    let selection = ObjectSelection::default();
    assert!(selection.is_empty());
    assert_eq!(selection.len(), 0);
    assert_eq!(selection.lead(), None);
}

#[test]
fn members_keep_insertion_order_and_the_lead_is_last() {
    let selection = selection_of(&[3, 1, 2]);
    assert_eq!(nodes(&selection), [node(3), node(1), node(2)]);
    assert_eq!(selection.lead(), Some(&plain(2)));
    assert_eq!(selection.len(), 3);
}

#[test]
fn replace_leaves_only_the_new_member() {
    let mut selection = selection_of(&[1, 2, 3]);
    selection.replace(plain(2));
    assert_eq!(selection.members(), [plain(2)]);
    assert_eq!(selection.lead(), Some(&plain(2)));
}

#[test]
fn add_of_an_existing_member_moves_it_to_the_end_and_leads() {
    let mut selection = selection_of(&[1, 2, 3]);
    selection.add(plain(1)).unwrap();
    assert_eq!(nodes(&selection), [node(2), node(3), node(1)]);
    assert_eq!(selection.lead(), Some(&plain(1)));
}

#[test]
fn identity_is_node_and_path_so_one_node_can_be_two_members() {
    let mut selection = ObjectSelection::default();
    selection.add(target(5, &[1, 5])).unwrap();
    selection.add(target(5, &[2, 5])).unwrap();
    selection.add(target(5, &[1, 5])).unwrap();
    assert_eq!(
        selection.members(),
        [target(5, &[2, 5]), target(5, &[1, 5])]
    );
    assert!(selection.contains(&target(5, &[2, 5])));
    assert!(!selection.contains(&target(5, &[3, 5])));
}

#[test]
fn toggle_adds_an_absent_member_as_the_lead() {
    let mut selection = selection_of(&[1]);
    assert_eq!(selection.toggle(plain(2)), Ok(true));
    assert_eq!(nodes(&selection), [node(1), node(2)]);
    assert_eq!(selection.lead(), Some(&plain(2)));
}

#[test]
fn toggle_removes_a_present_member_and_promotes_the_previous_lead() {
    let mut selection = selection_of(&[1, 2, 3]);
    assert_eq!(selection.toggle(plain(3)), Ok(false));
    assert_eq!(selection.lead(), Some(&plain(2)));
    assert_eq!(selection.toggle(plain(1)), Ok(false));
    assert_eq!(selection.members(), [plain(2)]);
}

#[test]
fn remove_of_the_lead_the_middle_and_the_only_member() {
    let mut selection = selection_of(&[1, 2, 3, 4]);
    assert!(selection.remove(&plain(4)));
    assert_eq!(selection.lead(), Some(&plain(3)));
    assert!(selection.remove(&plain(2)));
    assert_eq!(nodes(&selection), [node(1), node(3)]);
    assert_eq!(selection.lead(), Some(&plain(3)));
    assert!(selection.remove(&plain(3)));
    assert!(selection.remove(&plain(1)));
    assert_eq!(selection.lead(), None);
    assert!(selection.is_empty());
}

#[test]
fn remove_of_an_absent_member_reports_false_and_changes_nothing() {
    let mut selection = selection_of(&[1, 2]);
    let before = selection.clone();
    assert!(!selection.remove(&plain(9)));
    assert!(!selection.remove(&target(1, &[7, 1])));
    assert_eq!(selection, before);
}

#[test]
fn clear_empties_the_selection() {
    let mut selection = selection_of(&[1, 2]);
    selection.clear();
    assert!(selection.is_empty());
    assert_eq!(selection.lead(), None);
}

#[test]
fn retain_keeps_order_and_the_lead_is_the_last_survivor() {
    let mut selection = selection_of(&[1, 2, 3, 4]);
    selection.retain(|member| member.node != node(4) && member.node != node(2));
    assert_eq!(nodes(&selection), [node(1), node(3)]);
    assert_eq!(selection.lead(), Some(&plain(3)));
    selection.retain(|_| false);
    assert!(selection.is_empty());
}

fn full_selection() -> ObjectSelection {
    let mut selection = ObjectSelection::default();
    for value in 0..MAX_TRANSFORM_PATHS as u64 {
        selection.add(plain(value)).unwrap();
    }
    selection
}

#[test]
fn selection_cap_is_the_transform_path_cap() {
    assert_eq!(MAX_TRANSFORM_PATHS, 4096);
    assert_eq!(full_selection().len(), MAX_TRANSFORM_PATHS);
}

#[test]
fn add_past_the_cap_is_an_error_and_does_not_mutate() {
    let mut selection = full_selection();
    let before = selection.clone();
    let extra = plain(MAX_TRANSFORM_PATHS as u64);
    assert_eq!(selection.add(extra.clone()), Err(SelectionFull));
    assert_eq!(selection.toggle(extra), Err(SelectionFull));
    assert_eq!(selection, before);
}

#[test]
fn a_full_selection_still_moves_or_removes_existing_members() {
    let mut selection = full_selection();
    selection.add(plain(0)).unwrap();
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS);
    assert_eq!(selection.lead(), Some(&plain(0)));
    assert_eq!(selection.toggle(plain(0)), Ok(false));
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS - 1);
    assert_eq!(selection.add(plain(99_999)), Ok(()));
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS);
}

#[test]
fn selection_full_error_names_the_cap() {
    assert_eq!(
        SelectionFull.to_string(),
        "A selection holds at most 4096 objects"
    );
}
