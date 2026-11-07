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

#[test]
fn replace_with_sets_the_members_in_order_and_the_last_leads() {
    let mut selection = selection_of(&[9]);
    selection
        .replace_with(vec![plain(3), plain(1), plain(2)])
        .unwrap();
    assert_eq!(nodes(&selection), [node(3), node(1), node(2)]);
    assert_eq!(selection.lead(), Some(&plain(2)));
    selection.replace_with(Vec::new()).unwrap();
    assert!(selection.is_empty());
}

#[test]
fn replace_with_keeps_the_first_position_of_a_repeated_target() {
    let mut selection = ObjectSelection::default();
    selection
        .replace_with(vec![plain(1), plain(2), plain(1), target(2, &[7, 2])])
        .unwrap();
    assert_eq!(
        selection.members(),
        [plain(1), plain(2), target(2, &[7, 2])]
    );
}

#[test]
fn replace_with_over_the_cap_is_an_error_and_does_not_mutate() {
    let mut selection = selection_of(&[1, 2]);
    let before = selection.clone();
    let too_many = (0..=MAX_TRANSFORM_PATHS as u64).map(plain).collect();
    assert_eq!(selection.replace_with(too_many), Err(SelectionFull));
    assert_eq!(selection, before);
    let exactly = (0..MAX_TRANSFORM_PATHS as u64).map(plain).collect();
    selection.replace_with(exactly).unwrap();
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS);
}

#[test]
fn replace_with_counts_unique_targets_against_the_cap() {
    let mut selection = ObjectSelection::default();
    let mut repeated: Vec<_> = (0..MAX_TRANSFORM_PATHS as u64).map(plain).collect();
    repeated.push(plain(0));
    selection.replace_with(repeated).unwrap();
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS);
}

mod generation {
    use super::*;

    fn bumps(selection: &mut ObjectSelection, change: impl FnOnce(&mut ObjectSelection)) -> u64 {
        let before = selection.generation();
        change(selection);
        selection.generation().wrapping_sub(before)
    }

    #[test]
    fn each_mutator_bumps_exactly_once_when_it_changes_the_members() {
        let mut selection = selection_of(&[1, 2, 3]);
        assert_eq!(bumps(&mut selection, |s| s.add(plain(4)).unwrap()), 1);
        assert_eq!(bumps(&mut selection, |s| s.add(plain(1)).unwrap()), 1);
        assert_eq!(bumps(&mut selection, |s| assert!(s.remove(&plain(2)))), 1);
        assert_eq!(
            bumps(&mut selection, |s| s.retain(|t| t.node != node(3))),
            1
        );
        assert_eq!(bumps(&mut selection, |s| s.replace(plain(9))), 1);
        assert_eq!(
            bumps(&mut selection, |s| s
                .replace_with(vec![plain(5), plain(6)])
                .unwrap()),
            1
        );
        assert_eq!(
            bumps(&mut selection, |s| assert!(s.toggle(plain(7)).unwrap())),
            1
        );
        assert_eq!(
            bumps(&mut selection, |s| assert!(!s.toggle(plain(7)).unwrap())),
            1
        );
        assert_eq!(bumps(&mut selection, ObjectSelection::clear), 1);
    }

    #[test]
    fn toggle_swapping_a_non_lead_member_bumps_generation() {
        let mut selection = selection_of(&[1, 2, 3]);
        let before = selection.generation();
        selection.toggle(plain(2)).unwrap();
        selection.toggle(plain(4)).unwrap();
        assert_ne!(selection.generation(), before);
    }

    #[test]
    fn refused_and_no_op_edits_do_not_bump() {
        let mut selection = selection_of(&[1]);
        assert_eq!(bumps(&mut selection, |s| s.replace(plain(1))), 0);
        assert_eq!(bumps(&mut selection, |s| s.add(plain(1)).unwrap()), 0);
        assert_eq!(bumps(&mut selection, |s| assert!(!s.remove(&plain(8)))), 0);
        assert_eq!(bumps(&mut selection, |s| s.retain(|_| true)), 0);
        assert_eq!(
            bumps(&mut selection, |s| s.replace_with(vec![plain(1)]).unwrap()),
            0
        );
        let mut empty = ObjectSelection::default();
        assert_eq!(bumps(&mut empty, ObjectSelection::clear), 0);
        let mut full = full_selection();
        let over = (0..)
            .map(|i| plain(1_000_000 + i))
            .take(1)
            .collect::<Vec<_>>();
        assert_eq!(
            bumps(&mut full, |s| assert!(
                s.replace_with((0..=MAX_TRANSFORM_PATHS as u64).map(plain).collect())
                    .is_err()
            )),
            0
        );
        assert_eq!(
            bumps(&mut full, |s| assert!(s.add(over[0].clone()).is_err())),
            0
        );
    }

    #[test]
    fn equality_ignores_the_generation() {
        let mut a = selection_of(&[1, 2]);
        let b = selection_of(&[1, 2]);
        a.add(plain(3)).unwrap();
        a.remove(&plain(3));
        assert_ne!(a.generation(), b.generation());
        assert_eq!(a, b);
    }

    #[test]
    fn the_generation_wraps() {
        let mut selection = selection_of(&[1]);
        selection.generation = u64::MAX;
        selection.clear();
        assert_eq!(selection.generation(), 0);
    }
}

fn plains(values: &[u64]) -> Vec<TransformTarget> {
    values.iter().map(|value| plain(*value)).collect()
}

#[test]
fn extend_absent_appends_only_absent_targets_in_order_and_leads_with_the_last() {
    let mut selection = selection_of(&[1, 2]);
    let generation = selection.generation();
    selection
        .extend_absent(plains(&[5, 2, 4, 5, 1, 3]))
        .unwrap();
    assert_eq!(
        nodes(&selection),
        [node(1), node(2), node(5), node(4), node(3)]
    );
    assert_eq!(selection.lead(), Some(&plain(3)));
    assert_ne!(selection.generation(), generation);
}

#[test]
fn extend_absent_with_only_members_keeps_order_lead_and_generation() {
    let mut selection = selection_of(&[1, 2, 3]);
    let generation = selection.generation();
    selection.extend_absent(plains(&[1, 3])).unwrap();
    selection.extend_absent(Vec::new()).unwrap();
    assert_eq!(nodes(&selection), [node(1), node(2), node(3)]);
    assert_eq!(selection.generation(), generation);
}

#[test]
fn extend_absent_over_the_cap_changes_nothing() {
    let mut selection = full_selection();
    selection.remove(&plain(0));
    let before = selection.clone();
    let generation = selection.generation();
    let too_many = plains(&[10_000, 10_001]);
    assert_eq!(selection.extend_absent(too_many), Err(SelectionFull));
    assert_eq!(selection, before);
    assert_eq!(selection.generation(), generation);
    // One slot is free, and a repeated or already-present target needs none.
    selection
        .extend_absent(plains(&[10_000, 10_000, 1]))
        .unwrap();
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS);
    assert_eq!(selection.lead(), Some(&plain(10_000)));
}

#[test]
fn remove_all_removes_the_present_targets_and_promotes_the_previous_lead() {
    let mut selection = selection_of(&[1, 2, 3, 4]);
    let generation = selection.generation();
    assert!(selection.remove_all(&plains(&[4, 2, 9])));
    assert_eq!(nodes(&selection), [node(1), node(3)]);
    assert_eq!(selection.lead(), Some(&plain(3)));
    assert_ne!(selection.generation(), generation);
}

#[test]
fn remove_all_of_absent_targets_changes_nothing() {
    let mut selection = selection_of(&[1, 2]);
    let generation = selection.generation();
    assert!(!selection.remove_all(&plains(&[7, 8])));
    assert!(!selection.remove_all(&[]));
    assert_eq!(nodes(&selection), [node(1), node(2)]);
    assert_eq!(selection.generation(), generation);
}

#[test]
fn toggle_all_removes_members_and_appends_absent_targets_in_order() {
    let mut selection = selection_of(&[1, 2, 3]);
    selection.toggle_all(&plains(&[2, 7, 1, 6, 7])).unwrap();
    assert_eq!(nodes(&selection), [node(3), node(7), node(6)]);
    assert_eq!(selection.lead(), Some(&plain(6)));
}

#[test]
fn toggle_all_that_only_removes_promotes_the_previous_member() {
    let mut selection = selection_of(&[1, 2, 3]);
    selection.toggle_all(&plains(&[3])).unwrap();
    assert_eq!(selection.lead(), Some(&plain(2)));
    selection.toggle_all(&plains(&[2, 1])).unwrap();
    assert!(selection.is_empty());
}

#[test]
fn toggle_all_with_no_targets_keeps_the_generation() {
    let mut selection = selection_of(&[1, 2]);
    let generation = selection.generation();
    selection.toggle_all(&[]).unwrap();
    assert_eq!(nodes(&selection), [node(1), node(2)]);
    assert_eq!(selection.generation(), generation);
}

#[test]
fn toggle_all_over_the_cap_changes_nothing_but_removals_make_room() {
    let mut selection = full_selection();
    let before = selection.clone();
    let generation = selection.generation();
    assert_eq!(selection.toggle_all(&plains(&[10_000])), Err(SelectionFull));
    assert_eq!(selection, before);
    assert_eq!(selection.generation(), generation);
    // Removing one member frees the slot the new target needs.
    selection.toggle_all(&plains(&[0, 10_000])).unwrap();
    assert_eq!(selection.len(), MAX_TRANSFORM_PATHS);
    assert_eq!(selection.lead(), Some(&plain(10_000)));
    assert!(!selection.contains(&plain(0)));
}
