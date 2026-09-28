use super::*;
#[test]
fn reconstruction_is_charged_resumable_hashed_and_cost_equivalent() {
    let terrain = flat_terrain_with_width(128);
    let overlay = cleared_overlay_with_width(&terrain, 128);
    let origin = TileCoord::new(1, 1);
    let destination = TileCoord::new(100, 1);
    let mut one_unit_at_a_time = RoutePlanner::new(origin, destination, 16_384);
    let mut segmented_tiles = Vec::new();
    let mut segmented_cost = 0_u64;
    let mut saw_reconstruction = false;
    let mut saw_reconstruction_pending = false;
    let mut polls = 0;
    loop {
        polls += 1;
        assert!(polls <= 16_384);
        let before_work = one_unit_at_a_time.work();
        let was_reconstructing = one_unit_at_a_time.is_reconstructing();
        let mut before_hash = blake3::Hasher::new();
        one_unit_at_a_time.update_hash(&mut before_hash);
        let outcome = one_unit_at_a_time.poll(&terrain, &overlay, 1, &|| false);
        assert!(one_unit_at_a_time.work().saturating_sub(before_work) <= 1);
        if was_reconstructing {
            saw_reconstruction = true;
            assert_eq!(one_unit_at_a_time.work() - before_work, 1);
            if matches!(outcome, RoutePlannerPoll::Pending) {
                saw_reconstruction_pending = true;
                let mut after_hash = blake3::Hasher::new();
                one_unit_at_a_time.update_hash(&mut after_hash);
                assert_ne!(before_hash.finalize(), after_hash.finalize());
            }
        }
        if one_unit_at_a_time.is_reconstructing() {
            saw_reconstruction = true;
        }
        match outcome {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                if !segmented_tiles.is_empty() {
                    assert_eq!(path.tiles.first(), segmented_tiles.last());
                    segmented_tiles.extend(path.tiles.into_iter().skip(1));
                } else {
                    segmented_tiles.extend(path.tiles);
                }
                segmented_cost = segmented_cost.saturating_add(path.cost);
                if one_unit_at_a_time.is_terminal() {
                    break;
                }
            }
            other => panic!("one-unit route failed: {other:?}"),
        }
    }
    assert!(saw_reconstruction);
    assert!(saw_reconstruction_pending);
    assert_eq!(segmented_tiles.first(), Some(&origin));
    assert_eq!(segmented_tiles.last(), Some(&destination));
    let expected_cost = segmented_tiles.windows(2).fold(0_u64, |total, pair| {
        let base = if pair[0].x == pair[1].x || pair[0].y == pair[1].y {
            super::ORTHOGONAL_COST
        } else {
            super::DIAGONAL_COST
        };
        let multiplier = terrain
            .tile_at(pair[1])
            .map(|tile| tile.material == crate::GroundMaterial::Mud)
            .unwrap_or(false);
        let step = if multiplier { base * 3 / 2 } else { base };
        total.saturating_add(u64::from(step))
    });
    assert_eq!(segmented_cost, expected_cost);
    assert!(one_unit_at_a_time.peak_retained_entries() <= MAX_ROUTE_PLANNER_NODES);

    let mut one_poll = RoutePlanner::new(origin, destination, 16_384);
    let mut one_poll_tiles = Vec::new();
    let mut one_poll_cost = 0_u64;
    for _ in 0..16_384 {
        match one_poll.poll(&terrain, &overlay, 16_384, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                if one_poll_tiles.is_empty() {
                    one_poll_tiles.extend(path.tiles);
                } else {
                    assert_eq!(path.tiles.first(), one_poll_tiles.last());
                    one_poll_tiles.extend(path.tiles.into_iter().skip(1));
                }
                one_poll_cost = one_poll_cost.saturating_add(path.cost);
                if one_poll.is_terminal() {
                    break;
                }
            }
            other => panic!("single-poll route failed: {other:?}"),
        }
    }
    assert!(one_poll.is_terminal());
    assert_eq!(segmented_tiles, one_poll_tiles);
    assert_eq!(segmented_cost, one_poll_cost);
    let mut segmented_hash = blake3::Hasher::new();
    one_unit_at_a_time.update_hash(&mut segmented_hash);
    let mut one_poll_hash = blake3::Hasher::new();
    one_poll.update_hash(&mut one_poll_hash);
    assert_eq!(segmented_hash.finalize(), one_poll_hash.finalize());
}

#[test]
fn reconstruction_cancellation_discards_partial_route_state() {
    use std::cell::Cell;

    let terrain = flat_terrain_with_width(128);
    let overlay = cleared_overlay_with_width(&terrain, 128);
    let mut planner = RoutePlanner::new(TileCoord::new(1, 1), TileCoord::new(80, 1), 8_192);
    for _ in 0..8_192 {
        if planner.is_reconstructing() {
            break;
        }
        assert_eq!(
            planner.poll(&terrain, &overlay, 1, &|| false),
            RoutePlannerPoll::Pending
        );
    }
    assert!(planner.is_reconstructing());
    for _ in 0..3 {
        let before_work = planner.work();
        assert_eq!(
            planner.poll(&terrain, &overlay, 1, &|| false),
            RoutePlannerPoll::Pending
        );
        assert_eq!(planner.work() - before_work, 1);
        assert!(planner.is_reconstructing());
    }
    let before_work = planner.work();
    let checks = Cell::new(0);
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| {
            let checked = checks.get();
            checks.set(checked + 1);
            checked > 0
        }),
        RoutePlannerPoll::Environment(EnvironmentPageError::Cancelled)
    );
    assert_eq!(planner.work(), before_work);
    assert!(!planner.is_reconstructing());
    assert!(planner.is_terminal());
}

#[test]
fn constructor_never_allows_a_work_cap_above_the_published_limit() {
    let origin = TileCoord::new(1, 1);
    let destination = TileCoord::new(20, 1);
    let clamped = RoutePlanner::new(
        origin,
        destination,
        MAX_ROUTE_PLANNER_WORK.saturating_add(1),
    );
    let at_limit = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let mut clamped_hash = blake3::Hasher::new();
    clamped.update_hash(&mut clamped_hash);
    let mut limit_hash = blake3::Hasher::new();
    at_limit.update_hash(&mut limit_hash);
    assert_eq!(clamped_hash.finalize(), limit_hash.finalize());
}

#[test]
fn zero_hop_reconstruction_completes_at_the_exact_work_cap() {
    let terrain = flat_terrain();
    let overlay = cleared_overlay(&terrain);
    let origin = TileCoord::new(1, 1);
    let mut planner = RoutePlanner::new(origin, origin, 3);
    assert_eq!(
        planner.poll(&terrain, &overlay, 8, &|| false),
        RoutePlannerPoll::Path(crate::Path {
            tiles: vec![origin],
            cost: 0,
        })
    );
    assert_eq!(planner.work(), 3);
    assert!(planner.is_terminal());
}
