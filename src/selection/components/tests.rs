use super::*;

fn id(value: u64) -> NativeId {
    NativeId::from_decimal(&value.to_string()).unwrap()
}

fn owner(value: u64) -> ComponentOwner {
    ComponentOwner {
        transform: NodeId::from_decimal(&value.to_string()).unwrap(),
        shape: id(value + 100),
        path: vec![id(value), id(value + 100)],
    }
}

fn set_of(indices: &[usize]) -> ComponentSet {
    let mut set = ComponentSet::default();
    set.replace_with(&owner(1), indices.iter().copied());
    set
}

#[test]
fn replace_keeps_first_position_of_repeats_and_the_last_as_lead() {
    let set = set_of(&[3, 1, 3, 2, 1]);
    assert_eq!(set.indices(), [3, 1, 2]);
    assert_eq!(set.lead(), Some(2));
    assert_eq!(set.len(), 3);
    assert!(set.contains(1) && !set.contains(9));
    assert!(set.is_owned_by(&owner(1)));
}

#[test]
fn replace_reports_change_only_when_owner_or_order_differ() {
    let mut set = set_of(&[1, 2]);
    assert!(!set.replace_with(&owner(1), [1, 2]));
    assert!(set.replace_with(&owner(1), [2, 1]));
    assert!(set.replace_with(&owner(2), [2, 1]));
    assert!(set.replace_with(&owner(2), []));
    assert!(set.is_empty() && set.owner().is_none());
    assert!(!set.replace_with(&owner(2), []));
}

#[test]
fn extend_appends_absent_in_order_and_keeps_members_in_place() {
    let mut set = set_of(&[1, 2]);
    assert!(set.extend_absent(&owner(1), [3, 1, 4, 3]));
    assert_eq!(set.indices(), [1, 2, 3, 4]);
    assert!(!set.extend_absent(&owner(1), [2, 4]));
    assert_eq!(set.lead(), Some(4));
}

#[test]
fn extend_with_another_owner_drops_the_old_members() {
    let mut set = set_of(&[1, 2]);
    assert!(set.extend_absent(&owner(2), [7]));
    assert_eq!(set.indices(), [7]);
    assert!(set.is_owned_by(&owner(2)));
    assert!(set.extend_absent(&owner(1), []));
    assert!(set.is_empty() && set.owner().is_none());
}

#[test]
fn remove_drops_members_promotes_the_previous_lead_and_ignores_strangers() {
    let mut set = set_of(&[1, 2, 3]);
    assert!(set.remove_all(&owner(1), [3, 9]));
    assert_eq!(set.indices(), [1, 2]);
    assert_eq!(set.lead(), Some(2));
    assert!(!set.remove_all(&owner(1), [9]));
    assert!(!set.remove_all(&owner(2), [1]));
    assert_eq!(set.indices(), [1, 2]);
    assert!(set.remove_all(&owner(1), [1, 2]));
    assert!(set.is_empty() && set.owner().is_none());
}

#[test]
fn toggle_flips_each_distinct_index_once() {
    let mut set = set_of(&[1, 2, 3]);
    assert!(set.toggle_all(&owner(1), [2, 4, 2, 5, 4]));
    assert_eq!(set.indices(), [1, 3, 4, 5]);
    assert!(!set.contains(2));
    assert!(!set.toggle_all(&owner(1), []));
    assert!(set.toggle_all(&owner(1), [1, 3, 4, 5]));
    assert!(set.is_empty() && set.owner().is_none());
}

#[test]
fn toggle_with_another_owner_starts_a_new_set() {
    let mut set = set_of(&[1, 2]);
    assert!(set.toggle_all(&owner(2), [2]));
    assert_eq!(set.indices(), [2]);
    assert!(set.is_owned_by(&owner(2)));
}

#[test]
fn retain_keeps_order_and_empties_the_owner() {
    let mut set = set_of(&[5, 1, 4]);
    assert!(set.retain(|index| index != 1));
    assert_eq!(set.indices(), [5, 4]);
    assert!(!set.contains(1));
    assert!(!set.retain(|_| true));
    assert!(set.retain(|_| false));
    assert!(set.is_empty() && set.owner().is_none());
}

#[test]
fn equality_compares_owner_and_order() {
    assert_eq!(set_of(&[1, 2]), set_of(&[1, 2]));
    assert_ne!(set_of(&[1, 2]), set_of(&[2, 1]));
    let mut other = ComponentSet::default();
    other.replace_with(&owner(2), [1, 2]);
    assert_ne!(set_of(&[1, 2]), other);
}

#[test]
fn a_large_batch_is_linear() {
    let mut set = ComponentSet::default();
    set.replace_with(&owner(1), 0..200_000);
    assert!(set.toggle_all(&owner(1), (0..200_000).step_by(2)));
    assert_eq!(set.len(), 100_000);
    assert!(set.remove_all(&owner(1), (1..200_000).step_by(2)));
    assert!(set.is_empty());
}
