//! On France, stragglers stood for ever on a diagonal deck, re-planning every
//! tick: the route answered at once, but the straight step from a body beside the
//! deck's centre line to the route's next tile centre was refused.
use super::*;

#[test]
fn a_body_off_a_diagonal_decks_lane_walks_off_the_deck() {
    let mut world = river_width(true, 30..35);
    clear_nodes(&mut world, TileRect::from_xywh(24, 12, 20, 20));
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let (from, to) = (TileCoord::new(29, 20), TileCoord::new(35, 26));
    let span = aoe_core::BridgeSpan::new(from, to).unwrap();
    world.place_finished_bridge(span).unwrap();
    world.prepare_navigation(&|| false).unwrap();
    let goal = WorldPosition::from_tile_center(TileCoord::new(38, 28)).unwrap();
    let mut tried = 0;
    for tile in span.tiles() {
        for dy in (32..1024).step_by(124) {
            for dx in (32..1024).step_by(124) {
                let at = WorldPosition::new(tile.x * 1024 + dx, tile.y * 1024 + dy);
                if !world.body_position_clear(UnitKind::Paladin, at) {
                    continue;
                }
                let Ok(id) = world.spawn_kind(PlayerId(0), UnitKind::Paladin, at) else {
                    continue;
                };
                tried += 1;
                world.issue_move(id, goal).unwrap();
                advance(&mut world, 400);
                let unit = world.unit(id).unwrap();
                assert_eq!(unit.position, goal, "from {at:?}: {unit:?}");
                world.remove_unit(id);
            }
        }
    }
    assert!(tried > 40, "{tried}");
}

#[test]
fn bridge_order_snaps_a_start_one_tile_inland_or_in_the_river() {
    for horizontal in [false, true] {
        for start in [28, 30] {
            let (mut world, worker) = setup_axis(horizontal);
            // The inland misclick cannot be occupied by its own builder.
            world
                .issue_move(
                    worker,
                    WorldPosition::from_tile_center(if horizontal {
                        TileCoord::new(27, 20)
                    } else {
                        TileCoord::new(20, 27)
                    })
                    .unwrap(),
                )
                .unwrap();
            advance(&mut world, 40);
            let tile = |axis| {
                if horizontal {
                    TileCoord::new(axis, 20)
                } else {
                    TileCoord::new(20, axis)
                }
            };
            let id = world
                .issue_build_bridge(PlayerId(0), &[worker], tile(start), tile(34), false)
                .unwrap();
            assert_eq!(
                world.building(id).unwrap().bridge_span,
                aoe_core::BridgeSpan::new(tile(29), tile(34))
            );
        }
    }
}
