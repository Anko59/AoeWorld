use super::*;
use crate::Terrain;
use crate::game_world::terrain_fixtures::build;

#[test]
fn harvesting_a_walking_zone_keeps_the_field_and_the_patch_assignment() {
    let mut world = build("general-forest").world;
    let player = PlayerId(0);
    let sources = vec![
        (EntityId(1), TileRect::from_xywh(61, 27, 4, 4)),
        (EntityId(2), TileRect::from_xywh(97, 27, 4, 4)),
    ];
    let Terrain::Map { overlay, .. } = &world.terrain else {
        unreachable!()
    };
    let harvested = overlay
        .starting_nodes()
        .find(|node| node.tile == TileCoord::new(81, 29))
        .unwrap();
    let remaining = TileCoord::new(82, 29);
    let before = install(&mut world, player, &sources);
    assert_eq!(
        before.patch_owner(harvested.tile),
        before.patch_owner(remaining)
    );
    let owner = before.patch_owner(remaining).expect("forest has a Steward");
    world
        .prepare_resource_depletion(harvested.id, harvested.initial_amount)
        .unwrap()
        .commit();
    assert!(
        refresh(&mut world, player).is_none(),
        "a harvest rebuilt the zone"
    );
    assert_eq!(before.patch_owner(remaining), Some(owner));
}

#[test]
fn a_local_walking_edit_reuses_the_other_offices_field() {
    let mut world = build("river").world;
    let player = PlayerId(0);
    let sources = vec![
        (EntityId(1), TileRect::from_xywh(20, 35, 4, 4)),
        (EntityId(2), TileRect::from_xywh(230, 200, 4, 4)),
    ];
    let before = install(&mut world, player, &sources);
    world.management_ground_changed(TileRect::from_xywh(21, 42, 1, 1));
    let after = refresh(&mut world, player).unwrap();
    assert!(!Arc::ptr_eq(
        &before.fields[0].active.field,
        &after.fields[0].active.field
    ));
    assert!(Arc::ptr_eq(
        &before.fields[1].active.field,
        &after.fields[1].active.field
    ));
    assert!(Arc::ptr_eq(
        &before.fields[1].active.claims,
        &after.fields[1].active.claims
    ));
    assert!(!after.zone_changed(sources[1].0));
}

#[test]
fn a_recomputed_field_with_the_same_costs_moves_no_overlapping_zone() {
    let mut world = build("river").world;
    let player = PlayerId(0);
    // Boxes 30 tiles apart overlap almost entirely.
    let sources = vec![
        (EntityId(1), TileRect::from_xywh(20, 35, 4, 4)),
        (EntityId(2), TileRect::from_xywh(50, 35, 4, 4)),
    ];
    let before = install(&mut world, player, &sources);
    // A notice for ground that did not change rebuilds the field it falls
    // in; its costs and steps come out the same, so no zone moved.
    world.management_ground_changed(TileRect::from_xywh(21, 42, 1, 1));
    let after = refresh(&mut world, player).unwrap();
    assert!(!Arc::ptr_eq(
        &before.fields[0].active.field,
        &after.fields[0].active.field
    ));
    for (id, _) in &sources {
        assert!(!after.zone_changed(*id), "office {id:?} re-labelled");
    }
}

#[test]
fn a_walking_zone_respects_walls_and_its_owners_gate() {
    use crate::BuildingKind;
    use aoe_core::{Seed, WorldConfig};

    let mut world = GameWorld::new(WorldConfig::new(32, 32, Seed(1)).unwrap()).unwrap();
    let player = PlayerId(0);
    let sources = vec![(EntityId(1000), TileRect::from_xywh(7, 12, 4, 4))];
    let far = TileCoord::new(20, 16);
    let before = install(&mut world, player, &sources);
    assert!(before.owns(sources[0].0, far));

    for y in (0..32).filter(|y| !(12..16).contains(y)) {
        world
            .insert_building(
                player,
                BuildingKind::StoneWall,
                TileRect::from_xywh(15, y, 1, 1),
                true,
            )
            .unwrap();
    }
    let gate = world
        .insert_building(
            player,
            BuildingKind::StoneGate,
            TileRect::from_xywh(15, 12, 1, 4),
            true,
        )
        .unwrap();
    let through_gate = refresh(&mut world, player).unwrap();
    assert!(through_gate.owns(sources[0].0, far));
    world.set_gate_locked(player, gate, true).unwrap();
    let locked = refresh(&mut world, player).unwrap();
    assert!(!locked.owns(EntityId(1000), far));
    assert_ne!(locked.shape(sources[0].0), through_gate.shape(sources[0].0));
}

fn install(
    world: &mut GameWorld,
    player: PlayerId,
    sources: &[(EntityId, TileRect)],
) -> Arc<Walking> {
    for &(id, rect) in sources {
        world.economy.walking.stewards.set(player, id, Some(rect));
    }
    refresh(world, player).unwrap()
}

fn refresh(world: &mut GameWorld, player: PlayerId) -> Option<Arc<Walking>> {
    let key = Groups::key(player);
    let cache = &world.economy.walking;
    let next = cache.stewards.refresh(world, key, cache)?;
    world.economy.walking.stewards.publish(key, next.clone());
    Some(next)
}

#[test]
fn changed_walking_zones_refresh_once_a_period_at_each_players_phase() {
    use aoe_core::{Seed, WorldConfig};

    let mut world = GameWorld::new(WorldConfig::new(32, 32, Seed(1)).unwrap()).unwrap();
    let period = world.config.tick_hz;
    let source = TileRect::from_xywh(7, 12, 4, 4);
    for player in [PlayerId(3), PlayerId(7), PlayerId(22)] {
        install(
            &mut world,
            player,
            &[(EntityId(u32::from(player.0)), source)],
        );
    }
    world.management_barrier_changed(source);

    let mut refreshed = Vec::new();
    for tick in 20..20 + 8 * u64::from(period) {
        for (_, player) in world.economy.walking.stewards.due(tick, period) {
            if refresh(&mut world, player).is_some() {
                refreshed.push((tick, player));
            }
        }
    }
    // One look every eight game seconds, in the second of the player's slot.
    assert_eq!(
        refreshed,
        [(64, PlayerId(3)), (124, PlayerId(22)), (148, PlayerId(7))]
    );
}

#[test]
fn a_new_office_refreshes_its_walking_zone_within_a_second() {
    use aoe_core::{Seed, WorldConfig};

    let mut world = GameWorld::new(WorldConfig::new(32, 32, Seed(1)).unwrap()).unwrap();
    let period = world.config.tick_hz;
    let player = PlayerId(7);
    install(
        &mut world,
        player,
        &[(EntityId(1), TileRect::from_xywh(7, 12, 4, 4))],
    );
    world.economy.walking.stewards.set(
        player,
        EntityId(2),
        Some(TileRect::from_xywh(20, 12, 4, 4)),
    );

    let due: Vec<_> = (20..30)
        .filter(|&tick| !world.economy.walking.stewards.due(tick, period).is_empty())
        .collect();
    assert_eq!(due, [28]);
}

#[test]
fn a_distant_bridge_leaves_a_zone_on_its_period() {
    use aoe_core::{Seed, WorldConfig};

    let mut world = GameWorld::new(WorldConfig::new(512, 512, Seed(1)).unwrap()).unwrap();
    let period = world.config.tick_hz;
    let player = PlayerId(3);
    install(
        &mut world,
        player,
        &[(EntityId(3), TileRect::from_xywh(7, 12, 4, 4))],
    );
    world.management_ground_changed(TileRect::from_xywh(400, 400, 4, 4));

    // Looked at once for the bridge, then only on the player's turn (review
    // F1/F2 on #1009: the bridge flag outlived a look that found nothing).
    let mut looks = Vec::new();
    for tick in 20..20 + 8 * u64::from(period) {
        for (_, due) in world.economy.walking.stewards.due(tick, period) {
            looks.push(tick);
            refresh(&mut world, due);
        }
    }
    assert_eq!(looks, [24, 64]);
}

#[test]
fn walking_refresh_phases_survive_a_tick_rate_change() {
    use aoe_core::{Seed, WorldConfig};

    let mut world = GameWorld::new(WorldConfig::new(32, 32, Seed(1)).unwrap()).unwrap();
    let player = PlayerId(17);
    let source = TileRect::from_xywh(7, 12, 4, 4);
    install(&mut world, player, &[(EntityId(17), source)]);
    world.set_tick_hz(4);
    world.management_barrier_changed(source);

    let mut refreshed = Vec::new();
    // Game second 17 is player 17's turn of the eight-second period.
    for tick in 68..72 {
        for (_, player) in world
            .economy
            .walking
            .stewards
            .due(tick, world.config.tick_hz)
        {
            if refresh(&mut world, player).is_some() {
                refreshed.push((tick, player));
            }
        }
    }
    assert_eq!(refreshed, [(71, player)]);
}

#[test]
fn removing_a_walking_source_reassigns_the_whole_patch() {
    let mut world = build("general-forest").world;
    let player = PlayerId(0);
    let sources = [
        (EntityId(1), TileRect::from_xywh(61, 27, 4, 4)),
        (EntityId(2), TileRect::from_xywh(97, 27, 4, 4)),
    ];
    let before = install(&mut world, player, &sources);
    let left = TileCoord::new(75, 29);
    let right = TileCoord::new(86, 29);
    let old_owner = before.patch_owner(left).unwrap();
    assert_eq!(before.patch_owner(right), Some(old_owner));
    let survivor = sources.iter().position(|&(id, _)| id != old_owner).unwrap();

    world.economy.walking.stewards.set(player, old_owner, None);
    let after = refresh(&mut world, player).unwrap();
    assert!(Arc::ptr_eq(
        &before.fields[survivor].active.field,
        &after.fields[0].active.field
    ));
    assert!(Arc::ptr_eq(
        &before.fields[survivor].active.claims,
        &after.fields[0].active.claims
    ));
    assert_eq!(after.patch_owner(left), Some(sources[survivor].0));
    assert_eq!(after.patch_owner(right), Some(sources[survivor].0));
}

#[test]
fn an_enemy_gate_keeps_a_walking_zone_out_open_or_closed() {
    use crate::BuildingKind;
    use aoe_core::{Seed, WorldConfig};

    let mut world = GameWorld::new(WorldConfig::new(32, 32, Seed(1)).unwrap()).unwrap();
    for y in (0..32).filter(|y| !(12..16).contains(y)) {
        world
            .insert_building(
                PlayerId(0),
                BuildingKind::StoneWall,
                TileRect::from_xywh(15, y, 1, 1),
                true,
            )
            .unwrap();
    }
    let gate = world
        .insert_building(
            PlayerId(0),
            BuildingKind::StoneGate,
            TileRect::from_xywh(15, 12, 1, 4),
            true,
        )
        .unwrap();
    let player = PlayerId(1);
    let office = EntityId(1000);
    let far = TileCoord::new(20, 16);
    let closed = install(
        &mut world,
        player,
        &[(office, TileRect::from_xywh(7, 12, 4, 4))],
    );
    assert!(!closed.owns(office, far));

    // The gate opens only while its owner's units pass (#978: following it
    // rebuilt every other player's zones around it on each toggle).
    world.restore_gate(gate, true);
    assert!(refresh(&mut world, player).is_none_or(|open| {
        !open.owns(office, far)
            && Arc::ptr_eq(&closed.fields[0].active.field, &open.fields[0].active.field)
    }));
}

#[test]
fn a_repaired_walking_field_matches_a_fresh_one_as_walls_rise_and_fall() {
    use crate::BuildingKind;

    let mut world = build("river").world;
    let player = PlayerId(0);
    let origin = TileRect::from_xywh(20, 35, 4, 4);
    let first = install(&mut world, player, &[(EntityId(1), origin)]);
    let fresh = |world: &GameWorld| {
        let bounds = Field::bounds(world, origin);
        Field::new(world, field::TerrainKey::new(world, player, origin, bounds)).unwrap()
    };
    let same = |a: &Field, b: &Field| {
        let bounds = b.terrain.bounds;
        assert_eq!(a.terrain.bounds, bounds);
        let differing = field::tiles(bounds)
            .filter(|&tile| a.cost(tile) != b.cost(tile))
            .count();
        assert_eq!(differing, 0, "tiles whose walk cost differs");
    };
    let walls: Vec<_> = (28..48)
        .filter_map(|y| {
            world
                .insert_building(
                    player,
                    BuildingKind::StoneWall,
                    TileRect::from_xywh(30, y, 1, 1),
                    true,
                )
                .ok()
        })
        .collect();
    assert!(walls.len() > 10, "the fixture has land for the wall");
    let walled = refresh(&mut world, player).unwrap();
    assert!(!Arc::ptr_eq(
        &first.fields[0].active.field,
        &walled.fields[0].active.field
    ));
    same(&walled.fields[0].active.field, &fresh(&world));

    // Half the wall falls: a state no earlier field was built for.
    for &id in walls.iter().step_by(2) {
        world.remove_building(id);
    }
    let breached = refresh(&mut world, player).unwrap();
    assert!(!Arc::ptr_eq(
        &first.fields[0].active.field,
        &breached.fields[0].active.field
    ));
    same(&breached.fields[0].active.field, &fresh(&world));
}

#[test]
fn walking_region_labels_respect_the_boundary_and_both_directions() {
    use aoe_map::clipped_region_labels;

    // Each row is a three-tile horizontal run. The right tiles also have a
    // south edge, but only the second of those edges has a reverse step.
    let west = 1 << 3;
    let east = 1 << 4;
    let north = 1 << 1;
    let south = 1 << 6;
    let mut steps = [
        Some(east),
        Some(west | east),
        Some(west | south),
        Some(east),
        Some(west | east),
        Some(west | south),
        Some(east),
        Some(west | east),
        Some(west | north),
    ];
    assert_eq!(
        clipped_region_labels(3, &steps),
        [1, 1, 1, 4, 4, 4, 4, 4, 4]
    );
    steps[1] = None;
    steps[5] = None;
    assert_eq!(
        clipped_region_labels(3, &steps),
        [1, 0, 3, 4, 4, 0, 7, 7, 7]
    );
}

#[test]
fn a_box_scan_of_ownership_matches_asking_tile_by_tile() {
    let mut world = build("general-forest").world;
    let player = PlayerId(0);
    let sources = [
        (EntityId(1), TileRect::from_xywh(61, 27, 4, 4)),
        (EntityId(2), TileRect::from_xywh(97, 27, 4, 4)),
        (EntityId(3), TileRect::from_xywh(80, 60, 4, 4)),
    ];
    let walking = install(&mut world, player, &sources);
    let mut contested = 0;
    for (index, &(id, _)) in sources.iter().enumerate() {
        let bounds = walking.fields[index].active.field.terrain.bounds;
        let scanned = walking.owned(index);
        for (tile, mine) in field::tiles(bounds).zip(scanned) {
            assert_eq!(mine, walking.owns(id, tile), "office {id:?} at {tile:?}");
            contested += usize::from(!mine && walking.owner(tile).is_some());
        }
    }
    assert!(contested > 0, "fixture: the boxes overlap");
}

#[test]
fn a_field_over_bounds_that_cut_through_its_office_does_not_index_past_them() {
    let world = build("river").world;
    let player = PlayerId(0);
    let origin = TileRect::from_xywh(20, 35, 4, 4);
    // A route guide's box need not hold its whole office: this one stops short of its last row.
    let bounds = TileRect::new(TileCoord::new(10, 30), TileCoord::new(22, 37));
    let field = Field::new(
        &world,
        field::TerrainKey::new(&world, player, origin, bounds),
    )
    .unwrap();
    assert!(field.cost(TileCoord::new(21, 36)).is_some());
    assert_eq!(field.cost(TileCoord::new(21, 37)), None);
}

#[test]
fn published_shape_matches_walking_ownership_across_the_river() {
    let mut world = build("river").world;
    let player = PlayerId(0);
    let office = EntityId(1);
    let before = install(
        &mut world,
        player,
        &[(office, TileRect::from_xywh(20, 35, 4, 4))],
    );
    let shape = before.shape(office).unwrap();
    let contains = |tile: TileCoord| {
        shape.iter().any(|(chunk, rows)| {
            (chunk.x, chunk.y) == page(tile, CHUNK_TILES)
                && rows[tile.y.rem_euclid(CHUNK_TILES) as usize]
                    & (1 << tile.x.rem_euclid(CHUNK_TILES))
                    != 0
        })
    };
    assert!(contains(TileCoord::new(22, 37)));
    for tile in field::tiles(TileRect::from_xywh(0, 0, 125, 140)) {
        assert_eq!(contains(tile), before.owns(office, tile), "{tile:?}");
    }
    world.management_ground_changed(TileRect::from_xywh(21, 42, 1, 1));
    let after = refresh(&mut world, player).unwrap();
    assert!(Arc::ptr_eq(&shape, &after.shape(office).unwrap()));
}
