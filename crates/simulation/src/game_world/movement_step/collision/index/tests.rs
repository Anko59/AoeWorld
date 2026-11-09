use super::*;

#[test]
fn cyclic_collision_samples_keep_order_and_the_actual_work_bound() {
    use crate::game_world::route_scheduler::GroupWork;
    let tile = TileCoord::new(2, 3);
    for size in [1, 7, 31, 32, 33, 65] {
        let mut index = CollisionIndex::default();
        for id in 0..size {
            index.insert(
                EntityId(id),
                WorldPosition::from_tile_center(tile).unwrap(),
                false,
                id as usize,
            );
        }
        for salt in [0, 1, 31, 32, u64::MAX] {
            let mut actual = Vec::new();
            let work = GroupWork::default();
            assert!(!index.blocked(
                (WorldPosition::from_tile_center(tile).unwrap(), 0),
                false,
                salt,
                &work,
                |id| {
                    actual.push(id);
                    false
                },
                |_| false
            ));
            let expected = (0..size.min(32))
                .map(|n| {
                    EntityId(((salt % u64::from(size) + u64::from(n)) % u64::from(size)) as u32)
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
            let mut metrics = crate::game_world::route_scheduler::RouteMetrics::default();
            work.snapshot(&mut metrics);
            assert_eq!(metrics.group_collision_work, 1 + u64::from(size.min(32)));
        }
    }
}

#[test]
fn a_window_rejects_outside_centres_without_a_body_test() {
    use crate::game_world::route_scheduler::GroupWork;
    let tile = TileCoord::new(2, 3);
    let centre = WorldPosition::from_tile_center(tile).unwrap();
    let mut index = CollisionIndex::default();
    index.insert(EntityId(0), centre, false, 0);
    // A step inside the tile moves the indexed centre out of the window.
    let moved = WorldPosition::new(centre.x + 100, centre.y);
    index.relocate(EntityId(0), tile, moved, false);
    assert_eq!(index.body_at(EntityId(0), tile, false), Some(moved));
    let tested = |window| {
        let mut tested = false;
        index.blocked_in(
            (centre, 0),
            window,
            false,
            0,
            &GroupWork::default(),
            |_| {
                tested = true;
                false
            },
            |_| false,
        );
        tested
    };
    assert!(!tested((centre.x, centre.x + 99, centre.y, centre.y)));
    assert!(tested((centre.x, centre.x + 100, centre.y, centre.y)));
}

/// The bound, in subunits, that the tests below arm the drift watch with
/// (`watch_drift` to `end_drift_watch`).
const P: i32 = 100;

fn centre(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn shifted(at: WorldPosition, dx: i32, dy: i32) -> WorldPosition {
    WorldPosition::new(at.x + dx, at.y + dy)
}

/// Land bodies with ids from 0, watched from where they stand.
fn watched(bodies: &[WorldPosition]) -> CollisionIndex {
    let mut index = CollisionIndex::default();
    for (id, &at) in bodies.iter().enumerate() {
        index.insert(EntityId(id as u32), at, false, id);
    }
    index.watch_drift(P);
    assert!(index.drift_within_watch(), "armed, and nothing moved");
    index
}

/// A unit's step as `track_unit_step` reports it: out of the tile it stood in.
fn step(index: &mut CollisionIndex, id: u32, from: WorldPosition, to: WorldPosition) {
    index.relocate(EntityId(id), from.tile_floor(), to, false);
    let now = index.body_at(EntityId(id), to.tile_floor(), false);
    assert_eq!(now, Some(to), "the index follows the step");
}

#[test]
fn a_body_farther_than_the_bound_from_where_it_was_armed_voids_the_watch_for_the_batch() {
    // The second body stands on the last subunit of its tile.
    let (a, b) = (centre(2, 3), WorldPosition::new(10 * 1024 - 1, 9728));
    // The bound itself is allowed. The distance is Euclidean: 60 and 80 make
    // 100, 70 and 71 make 99.7.
    for (dx, dy) in [(P, 0), (0, -P), (60, 80), (70, 71)] {
        let mut index = watched(&[a, b]);
        step(&mut index, 0, a, shifted(a, dx, dy));
        assert!(index.drift_within_watch(), "{dx}, {dy}");
    }
    // One subunit more is not: 71 and 71 are within the bound on each axis
    // and 100.4 from where the body was armed.
    for (dx, dy) in [(P + 1, 0), (0, -P - 1), (60, 81), (71, 71)] {
        let (far, near) = (shifted(a, dx, dy), shifted(b, 1, 0));
        let mut index = watched(&[a, b]);
        step(&mut index, 0, a, far);
        assert!(!index.drift_within_watch(), "{dx}, {dy}");
        // Void until the batch ends. Neither the body back where it was armed
        // nor another body's one-subunit step into the next tile restores it.
        step(&mut index, 0, far, a);
        assert!(!index.drift_within_watch(), "{dx}, {dy}: back");
        assert_ne!(b.tile_floor(), near.tile_floor(), "the scene: a new tile");
        step(&mut index, 1, b, near);
        assert!(!index.drift_within_watch(), "{dx}, {dy}: another step");
    }
}

#[test]
fn drift_adds_up_from_the_place_at_arming_whatever_the_single_steps() {
    // Both steps inside one tile, then the second one across a tile edge.
    for a in [centre(2, 3), WorldPosition::new(4 * 1024 - 80, 3584)] {
        let (once, twice) = (shifted(a, 60, 0), shifted(a, 120, 0));
        let mut index = watched(&[a]);
        step(&mut index, 0, a, once);
        assert!(index.drift_within_watch(), "{a:?}: 0.6 of the bound");
        step(&mut index, 0, once, twice);
        assert!(!index.drift_within_watch(), "{a:?}: 1.2 in two steps");
        // The displacement counts, not the distance walked: there and back
        // three times leaves the body where it was armed.
        let mut index = watched(&[a]);
        for _ in 0..3 {
            step(&mut index, 0, a, once);
            step(&mut index, 0, once, a);
        }
        assert!(index.drift_within_watch(), "{a:?}: 360 subunits, none kept");
    }
}

#[test]
fn a_step_across_a_tile_edge_is_measured_like_one_inside_a_tile() {
    // Thirty subunits inside tile (3, 3), at two of its corners. Tiles 2 and 3
    // share a coarse cell; tile 4 begins the next one.
    let low = WorldPosition::new(3 * 1024 + 30, 3 * 1024 + 30);
    let high = WorldPosition::new(4 * 1024 - 30, 4 * 1024 - 30);
    for (from, (dx, dy), within) in [
        (low, (-60, 0), true),
        (low, (-60, -60), true),
        (high, (60, 0), true),
        (high, (0, 60), true),
        (high, (80, 60), true),
        (high, (81, 60), false),
        (high, (P + 1, 0), false),
        (low, (0, -P - 1), false),
    ] {
        let to = shifted(from, dx, dy);
        assert_ne!(from.tile_floor(), to.tile_floor(), "the scene: a new tile");
        let mut index = watched(&[from, centre(9, 9)]);
        step(&mut index, 0, from, to);
        assert_eq!(index.drift_within_watch(), within, "{from:?} by {dx}, {dy}");
    }
    // The origin survives the change of cell: back across the edge is no
    // drift, and 60 more on the far side are 120 from the place at arming.
    let (over, further) = (shifted(high, 60, 0), shifted(high, 120, 0));
    let mut index = watched(&[high]);
    step(&mut index, 0, high, over);
    step(&mut index, 0, over, high);
    step(&mut index, 0, high, over);
    assert!(index.drift_within_watch(), "there, back and there again");
    step(&mut index, 0, over, further);
    assert!(!index.drift_within_watch(), "120 from where it was armed");
}

#[test]
fn a_new_body_voids_the_watch_and_a_removed_one_does_not() {
    let (a, b) = (centre(2, 3), centre(9, 9));
    let mut index = watched(&[a, b]);
    index.remove(EntityId(1), b.tile_floor(), false);
    assert_eq!(index.body_at(EntityId(1), b.tile_floor(), false), None);
    assert!(index.drift_within_watch(), "one body fewer closes no step");
    // The bodies left are still measured.
    step(&mut index, 0, a, shifted(a, P, 0));
    assert!(index.drift_within_watch(), "the bound, after a removal");
    step(&mut index, 0, shifted(a, P, 0), shifted(a, P + 1, 0));
    assert!(!index.drift_within_watch(), "beyond it, after a removal");
    // No certificate of the batch saw a new body, wherever it appears.
    let mut index = watched(&[a, b]);
    index.insert(EntityId(2), centre(200, 200), false, 2);
    assert!(!index.drift_within_watch(), "a new body, far from both");
    // Nor one that left the index and returns, even to its old place.
    let mut index = watched(&[a, b]);
    index.remove(EntityId(1), b.tile_floor(), false);
    index.insert(EntityId(1), b, false, 1);
    assert!(!index.drift_within_watch(), "removed and back");
    // `track_unit_step` inserts a body once more when its step crosses a
    // wildlife cell (eight tiles). The index holds it already: none appeared.
    let from = WorldPosition::new(8 * 1024 - 30, 3584);
    let to = shifted(from, 60, 0);
    let mut index = watched(&[from, b]);
    step(&mut index, 0, from, to);
    index.insert(EntityId(0), to, false, 0);
    assert!(index.drift_within_watch(), "an indexed body, once more");
}

#[test]
fn arming_again_measures_from_where_the_bodies_stand_then() {
    let at = |steps: i32| shifted(centre(2, 3), 60 * steps, 0);
    let mut index = watched(&[at(0)]);
    step(&mut index, 0, at(0), at(1));
    assert!(index.drift_within_watch(), "0.6 of the bound");
    index.end_drift_watch();
    assert!(!index.drift_within_watch(), "no batch runs");
    // What a body does between two batches is the drift of neither: the next
    // step ends three times the bound from where the first batch found it.
    step(&mut index, 0, at(1), at(4));
    index.watch_drift(P);
    assert!(index.drift_within_watch(), "a second batch");
    step(&mut index, 0, at(4), at(5));
    assert!(index.drift_within_watch(), "0.6 from the second arming");
    step(&mut index, 0, at(5), at(6));
    assert!(!index.drift_within_watch(), "1.2 from the second arming");
    // A void ends with its batch, and its origins with it: the next step ends
    // 1.8 times the bound from where the second batch found the body.
    index.end_drift_watch();
    index.watch_drift(P);
    assert!(index.drift_within_watch(), "a third batch starts whole");
    step(&mut index, 0, at(6), at(7));
    assert!(index.drift_within_watch(), "0.6 from the third arming");
    // The other way round: a step back is 1.2 times the bound from where a
    // fourth batch finds the body, and only 0.6 from the third's origin.
    index.end_drift_watch();
    index.watch_drift(P);
    step(&mut index, 0, at(7), at(5));
    assert!(!index.drift_within_watch(), "1.2 from the fourth arming");
}

#[test]
fn the_watch_vouches_for_nothing_unarmed_or_while_a_cell_is_overfull() {
    let a = centre(2, 3);
    let mut index = CollisionIndex::default();
    assert!(!index.drift_within_watch(), "empty, never armed");
    index.insert(EntityId(0), a, false, 0);
    step(&mut index, 0, a, shifted(a, 1, 0));
    assert!(!index.drift_within_watch(), "never armed");
    // A bound of zero is a batch without certificates: it arms nothing.
    index.watch_drift(0);
    assert!(!index.drift_within_watch(), "armed with no bound");
    index.watch_drift(P);
    assert!(index.drift_within_watch(), "armed");
    index.end_drift_watch();
    assert!(!index.drift_within_watch(), "ended");

    // Sweeps sample a tile from its 33rd centre on. Nothing vouches for a
    // step then, although no body left its place.
    for (bodies, within) in [(32_u32, true), (33, false)] {
        let mut index = CollisionIndex::default();
        for id in 0..bodies {
            index.insert(EntityId(id), a, false, id as usize);
        }
        index.watch_drift(P);
        assert_eq!(index.has_overfull_cells(), !within, "{bodies} centres");
        assert_eq!(index.drift_within_watch(), within, "{bodies} centres");
    }
    // The 33rd centre can arrive within the bound; when it leaves again the
    // watch goes on: it was suspended, not voided.
    let inside = WorldPosition::new(3 * 1024 + 30, 3584);
    let beside = shifted(inside, -60, 0);
    let mut crowd = vec![inside; 32];
    crowd.push(beside);
    let mut index = watched(&crowd);
    step(&mut index, 32, beside, inside);
    assert!(index.has_overfull_cells(), "the 33rd centre arrived");
    assert!(!index.drift_within_watch(), "a drift of 60, 33 centres");
    step(&mut index, 32, inside, beside);
    assert!(!index.has_overfull_cells(), "and left");
    assert!(index.drift_within_watch(), "32 centres again");
}

#[test]
fn a_step_the_index_cannot_follow_voids_the_watch() {
    let (a, elsewhere) = (centre(2, 3), centre(5, 5));
    // Reported out of a tile the body is not in: a step inside that tile,
    // then one that leaves it.
    for to in [shifted(elsewhere, 1, 0), centre(6, 5)] {
        let mut index = watched(&[a]);
        index.relocate(EntityId(0), elsewhere.tile_floor(), to, false);
        let kept = index.body_at(EntityId(0), a.tile_floor(), false);
        assert_eq!(kept, Some(a), "to {to:?}: the body stays where it was");
        assert!(!index.drift_within_watch(), "to {to:?}");
    }
}
