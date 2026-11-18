#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::Arc;

use super::*;

fn frame(bytes: usize) -> Arc<PlateFrame> {
    Arc::new(PlateFrame {
        width: 1,
        height: (bytes / 4) as u32,
        rgba: vec![0; bytes],
    })
}

fn key(frame: i64) -> FrameKey {
    FrameKey {
        generation: 1,
        frame,
    }
}

#[test]
fn effective_frames_follow_the_hand_values() {
    let budget = PLATE_CACHE_BUDGET_BYTES;
    // 4096 x 2160 x 4 = 35,389,440 B; 536,870,912 / 35,389,440 = 15.17, so 15.
    assert_eq!(effective_frames(24, budget, 4096 * 2160 * 4), 15);
    // 2K (2048 x 1080 x 4 = 8,847,360 B) would fit 60, so the default 24 caps it.
    assert_eq!(effective_frames(24, budget, 2048 * 1080 * 4), 24);
    assert_eq!(effective_frames(1, budget, 100), 1);
    // A frame bigger than the budget still keeps one.
    assert_eq!(effective_frames(24, 10, 100), 1);
    assert_eq!(effective_frames(24, 0, 0), 24);
}

#[test]
fn lru_evicts_the_oldest_and_get_refreshes() {
    let mut cache = FrameCache::new(1000, 3);
    for n in 1..=3 {
        cache.insert(key(n), frame(100), &[]).unwrap();
    }
    assert!(cache.get(key(1)).is_some());
    cache.insert(key(4), frame(100), &[]).unwrap();
    assert!(cache.contains(key(1)) && !cache.contains(key(2)));
    assert!(cache.contains(key(3)) && cache.contains(key(4)));
    assert_eq!((cache.len(), cache.total_bytes()), (3, 300));
}

#[test]
fn frame_cache_one_keeps_one() {
    let mut cache = FrameCache::new(1000, 1);
    cache.insert(key(1), frame(100), &[]).unwrap();
    cache.insert(key(2), frame(100), &[]).unwrap();
    assert!(!cache.contains(key(1)) && cache.contains(key(2)));
}

#[test]
fn pinned_entries_are_never_evicted() {
    let mut cache = FrameCache::new(1000, 2);
    cache.insert(key(1), frame(100), &[]).unwrap();
    cache.insert(key(2), frame(100), &[]).unwrap();
    cache.insert(key(3), frame(100), &[key(1)]).unwrap();
    assert!(cache.contains(key(1)) && !cache.contains(key(2)) && cache.contains(key(3)));
}

#[test]
fn the_byte_budget_evicts_before_the_frame_count() {
    let mut cache = FrameCache::new(250, 24);
    cache.insert(key(1), frame(100), &[]).unwrap();
    cache.insert(key(2), frame(100), &[]).unwrap();
    cache.insert(key(3), frame(100), &[]).unwrap();
    assert_eq!((cache.len(), cache.total_bytes()), (2, 200));
    assert_eq!(
        cache.insert(key(9), frame(251), &[]),
        Err(PlateError::TooLarge)
    );
    assert_eq!(cache.total_bytes(), 200);
}

#[test]
fn reinserting_a_key_replaces_it_without_double_counting() {
    let mut cache = FrameCache::new(1000, 4);
    cache.insert(key(1), frame(100), &[]).unwrap();
    cache.insert(key(1), frame(200), &[]).unwrap();
    assert_eq!((cache.len(), cache.total_bytes()), (1, 200));
}

#[test]
fn retain_generation_drops_older_sources() {
    let mut cache = FrameCache::new(1000, 4);
    cache.insert(key(1), frame(100), &[]).unwrap();
    let newer = FrameKey {
        generation: 2,
        frame: 1,
    };
    cache.insert(newer, frame(100), &[]).unwrap();
    cache.retain_generation(2);
    assert!(!cache.contains(key(1)) && cache.contains(newer));
    assert_eq!(cache.total_bytes(), 100);
}

#[test]
fn bytes_never_exceed_the_budget_over_a_seeded_insert_sequence() {
    let (budget, frame_cache) = (1000usize, 6usize);
    let mut cache = FrameCache::new(budget, frame_cache);
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..2000 {
        let frame_number = (next() % 40) as i64;
        let size = 50 + (next() % 200) as usize;
        let pinned = [key((next() % 40) as i64), key((next() % 40) as i64)];
        let _ = cache.insert(key(frame_number), frame(size), &pinned);
        assert!(
            cache.total_bytes() <= budget,
            "bytes {}",
            cache.total_bytes()
        );
        assert!(cache.len() <= frame_cache + pinned.len());
        let sum: usize = (0..40)
            .filter_map(|n| cache.entries.get(&key(n)))
            .map(|entry| entry.frame.rgba.len())
            .sum();
        assert_eq!(sum, cache.total_bytes());
    }
}

#[test]
fn an_insert_that_would_evict_a_pinned_entry_is_refused_and_changes_nothing() {
    let mut cache = FrameCache::new(1000, 2);
    cache.insert(key(1), frame(100), &[]).unwrap();
    cache.insert(key(2), frame(100), &[]).unwrap();
    let pinned = [key(1), key(2)];
    assert_eq!(
        cache.insert(key(3), frame(100), &pinned),
        Err(PlateError::TooLarge)
    );
    assert!(cache.contains(key(1)) && cache.contains(key(2)) && !cache.contains(key(3)));
    assert_eq!((cache.len(), cache.total_bytes()), (2, 200));
}

#[test]
fn a_refused_replacement_keeps_the_entry_and_the_byte_count() {
    let mut cache = FrameCache::new(400, 24);
    for n in 1..=3 {
        cache.insert(key(n), frame(100), &[]).unwrap();
    }
    let pinned = [key(1), key(2), key(3)];
    // The count fits; 200 + 250 does not, and every other entry is pinned.
    assert_eq!(
        cache.insert(key(3), frame(250), &pinned),
        Err(PlateError::TooLarge)
    );
    assert_eq!((cache.len(), cache.total_bytes()), (3, 300));
    assert_eq!(cache.get(key(3)).unwrap().rgba.len(), 100);
}

#[test]
fn the_byte_budget_refuses_when_everything_else_is_pinned() {
    let mut cache = FrameCache::new(350, 24);
    cache.insert(key(1), frame(200), &[]).unwrap();
    cache.insert(key(2), frame(100), &[]).unwrap();
    let pinned = [key(1), key(2)];
    assert_eq!(
        cache.insert(key(3), frame(100), &pinned),
        Err(PlateError::TooLarge)
    );
    assert!(!cache.contains(key(3)));
    assert_eq!((cache.len(), cache.total_bytes()), (2, 300));
}

#[test]
fn capacity_follows_the_first_frame_size() {
    let mut cache = FrameCache::new(400, 24);
    assert_eq!(cache.capacity(), 24);
    cache.insert(key(1), frame(100), &[]).unwrap();
    assert_eq!(cache.capacity(), 4);
}
