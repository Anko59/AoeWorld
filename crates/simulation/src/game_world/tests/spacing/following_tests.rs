//! Active follower windows and idle occupancy have independent work bounds.
use super::*;
use crate::game_world::spacing::{Followers, IdleSpacing};

#[test]
fn follower_cells_preserve_signed_eight_tile_boundaries() {
    let mut index = Followers::default();
    for (id, tile) in [-9, -8, -1, 0, 7, 8].into_iter().enumerate() {
        index.insert(EntityId(id as u32), TileCoord::new(tile, tile));
    }
    for (tile, expected) in [
        (-8, &[0, 1, 2][..]),
        (-1, &[1, 2, 3, 4][..]),
        (0, &[1, 2, 3, 4][..]),
        (8, &[3, 4, 5][..]),
    ] {
        let mut seen = BTreeSet::new();
        index.any_neighbour(TileCoord::new(tile, tile), 0, EntityId(u32::MAX), |id| {
            seen.insert(id);
            false
        });
        assert_eq!(
            seen,
            expected
                .iter()
                .copied()
                .map(EntityId)
                .collect::<BTreeSet<_>>()
        );
    }
}

fn candidates(index: &IdleSpacing) -> BTreeSet<EntityId> {
    (0..4).flat_map(|phase| index.candidates(phase)).collect()
}
#[test]
fn ordered_units_do_not_spend_the_idle_pass_budget() {
    let mut w = world();
    w.set_fog_disabled(true);
    for _ in 0..640 {
        w.spawn_villager(PlayerId(0), at(30, 30)).unwrap();
    }
    for n in 0..512 {
        w.issue_task_move(EntityId(n), at(40, 30)).unwrap();
    }
    for phase in 0..4 {
        let batch = w.idle_spacing.take_candidates(phase);
        assert_eq!(batch.len(), 32);
        assert!(batch.iter().all(|id| id.0 >= 512));
    }
    w.issue_stop(PlayerId(0), EntityId(0)).unwrap();
    assert!(candidates(&w.idle_spacing).contains(&EntityId(0)));
    w.issue_task_move(EntityId(0), at(40, 30)).unwrap();
    w.advance_movement();
    assert!(!candidates(&w.idle_spacing).contains(&EntityId(0)));
    w.issue_stop(PlayerId(0), EntityId(0)).unwrap();
    assert!(candidates(&w.idle_spacing).contains(&EntityId(0)));
}
#[test]
fn follower_neighbourhoods_sleep_and_wake_without_scanning_idle_units() {
    let mut index = Followers::default();
    let tile = TileCoord::new(30, 30);
    let east = TileCoord::new(31, 30);
    let far = TileCoord::new(50, 50);
    index.insert(EntityId(0), tile);
    index.any_neighbour(tile, 0, EntityId(0), |_| {
        panic!("only the querying actor is ordered")
    });
    index.insert(EntityId(1), east);
    let mut seen = BTreeSet::new();
    index.any_neighbour(tile, 0, EntityId(0), |id| {
        seen.insert(id);
        false
    });
    assert_eq!(seen, BTreeSet::from([EntityId(1)]));
    index.relocate(EntityId(1), east, far);
    index.any_neighbour(tile, 0, EntityId(0), |_| {
        panic!("ordered neighbour moved away")
    });
    index.relocate(EntityId(1), far, east);
    assert!(index.any_neighbour(tile, 0, EntityId(0), |id| id == EntityId(1)));
    index.remove(EntityId(1), east);
    index.any_neighbour(tile, 0, EntityId(0), |_| {
        panic!("neighbour stopped or was removed")
    });
}
#[test]
fn isolated_steps_reuse_the_empty_window_and_find_new_neighbours() {
    for (x, y, visits) in [
        (-1, -1, 5),
        (0, -1, 3),
        (1, -1, 5),
        (-1, 0, 3),
        (1, 0, 3),
        (-1, 1, 5),
        (0, 1, 3),
        (1, 1, 5),
    ] {
        let mut index = IdleSpacing::default();
        let from = TileCoord::new(30, 30);
        let to = TileCoord::new(30 + x, 30 + y);
        index.insert(EntityId(0), from);
        index.insert(EntityId(1), TileCoord::new(30 + 2 * x, 30 + 2 * y));
        let before = index.adjacent_visits;
        index.relocate(EntityId(0), from, to);
        assert_eq!(index.adjacent_visits - before, visits);
        assert_eq!(
            candidates(&index),
            BTreeSet::from([EntityId(0), EntityId(1)])
        );
        index.relocate(EntityId(0), to, from);
        assert!(candidates(&index).is_empty());
    }
}
#[test]
fn follower_membership_tracks_commands_completion_stop_and_death() {
    let mut w = world();
    w.set_fog_disabled(true);
    let id = w.spawn_unit(PlayerId(0), at(30, 30)).unwrap();
    assert!(!w.followers.contains(id));
    w.issue_move(id, at(32, 30)).unwrap();
    assert!(w.followers.contains(id));
    run(&mut w, 100);
    assert!(w.movement_order(id).is_none());
    assert!(!w.followers.contains(id));
    w.issue_move(id, at(60, 30)).unwrap();
    run(&mut w, 20);
    assert!(w.followers.contains(id));
    w.issue_stop(PlayerId(0), id).unwrap();
    assert!(!w.followers.contains(id));
    w.issue_move(id, at(70, 30)).unwrap();
    w.kill_unit(w.lookup[&id]);
    assert!(!w.followers.contains(id));
}
#[test]
fn tile_sample_order_is_pinned_by_the_world_hash() {
    for active in [true, false] {
        let mut w = world();
        let position = at(30, 30);
        for _ in 0..40 {
            w.spawn_villager(PlayerId(0), position).unwrap();
        }
        for n in 0..40 {
            w.issue_task_move(EntityId(n), at(50, 30)).unwrap();
        }
        let before = w.canonical_hash();
        // Both window orders affect future samples even with identical units.
        if active {
            w.followers.remove(EntityId(0), position.tile_floor());
            w.followers.insert(EntityId(0), position.tile_floor());
        } else {
            let unit = w.unit(EntityId(0)).unwrap();
            w.untrack_unit_cell(unit.id, crate::game_world::wildlife::cell(position));
            w.track_unit_cell(&unit);
        }
        assert_ne!(w.canonical_hash(), before, "active grid: {active}");
    }
}
#[test]
fn distant_idle_crowds_do_not_hide_a_close_moving_leader() {
    let mut w = world();
    w.set_fog_disabled(true);
    let position = at(30, 30);
    let follower = w.spawn_villager(PlayerId(0), position).unwrap();
    for _ in 0..64 {
        w.spawn_villager(PlayerId(0), at(27, 27)).unwrap();
    }
    let leader = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::new(position.x + 400, position.y),
        )
        .unwrap();
    for id in [follower, leader] {
        w.issue_task_move(id, WorldPosition::new(position.x + 20_000, position.y))
            .unwrap();
    }
    assert!(w.following_unit(w.lookup[&follower]));
    w.kill_unit(w.lookup[&leader]);
    assert!(!w.following_unit(w.lookup[&follower]));
}
#[test]
fn idle_occupancy_tracks_every_adjacent_direction_and_relocation() {
    let mut index = IdleSpacing::default();
    let tile = TileCoord::new(30, 30);
    let far = TileCoord::new(50, 50);
    index.insert(EntityId(0), tile);
    for y in -1..=1 {
        for x in -1..=1 {
            if (x, y) == (0, 0) {
                continue;
            }
            let other = TileCoord::new(tile.x + x, tile.y + y);
            index.insert(EntityId(1), other);
            assert_eq!(
                candidates(&index),
                BTreeSet::from([EntityId(0), EntityId(1)])
            );
            index.relocate(EntityId(1), other, far);
            assert!(candidates(&index).is_empty());
            index.remove(EntityId(1), far);
        }
    }
}
#[test]
fn follower_samples_are_bounded_repeatable_and_cyclic_in_dense_crowds() {
    let tile = TileCoord::new(32, 32);
    let mut index = Followers::default();
    for n in 0..20_000 {
        index.insert(EntityId(n), tile);
    }
    let sample = |index: &Followers, salt| {
        let mut ids = Vec::new();
        assert!(!index.any_neighbour(tile, salt, EntityId(u32::MAX), |id| {
            ids.push(id);
            false
        }));
        ids
    };
    for salt in [0, 1, 19_990, 20_000, u64::MAX] {
        let ids = sample(&index, salt);
        let start = (salt % 20_000) as u32;
        assert_eq!(
            ids,
            (0..32)
                .map(|n| EntityId((start + n) % 20_000))
                .collect::<Vec<_>>()
        );
        assert_eq!(ids, sample(&index, salt));
    }
    for (i, other) in [
        TileCoord::new(31, 31),
        TileCoord::new(32, 31),
        TileCoord::new(31, 32),
    ]
    .into_iter()
    .enumerate()
    {
        for n in 0..40 {
            index.insert(EntityId(20_000 + i as u32 * 40 + n), other);
        }
    }
    assert_eq!(sample(&index, 0).len(), 128, "four cells times 32");
}
#[test]
fn follower_slots_survive_swap_removal_and_relocation() {
    let tile = TileCoord::new(30, 30);
    let far = TileCoord::new(50, 50);
    let mut index = Followers::default();
    for n in 0..40 {
        index.insert(EntityId(n), tile);
    }
    index.remove(EntityId(0), tile);
    for id in [EntityId(39), EntityId(38)] {
        index.relocate(id, tile, far);
    }
    let mut seen = BTreeSet::new();
    index.any_neighbour(far, 0, EntityId(u32::MAX), |id| {
        seen.insert(id);
        false
    });
    assert_eq!(seen, BTreeSet::from([EntityId(38), EntityId(39)]));
    for salt in 0..40 {
        index.any_neighbour(tile, salt, EntityId(u32::MAX), |id| {
            assert!(![EntityId(0), EntityId(38), EntityId(39)].contains(&id));
            false
        });
    }
}
