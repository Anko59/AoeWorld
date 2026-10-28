//! Idle occupancy work bounds.
use super::*;
use crate::game_world::spacing::IdleSpacing;

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
fn tile_sample_order_is_pinned_by_the_world_hash() {
    {
        let mut w = world();
        let position = at(30, 30);
        for _ in 0..40 {
            w.spawn_villager(PlayerId(0), position).unwrap();
        }
        for n in 0..40 {
            w.issue_task_move(EntityId(n), at(50, 30)).unwrap();
        }
        let before = w.canonical_hash();
        // The window order affects future samples even with identical units.
        let unit = w.unit(EntityId(0)).unwrap();
        w.untrack_unit_cell(unit.id, crate::game_world::wildlife::cell(position));
        w.track_unit_cell(&unit);
        assert_ne!(w.canonical_hash(), before);
    }
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
