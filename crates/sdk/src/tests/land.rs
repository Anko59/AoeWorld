use super::*;
use aoe_map::{CompactChunk, MapChunkGenerator, SurfaceKind, WaterKind};
use std::sync::Arc;

#[test]
fn bridge_observations_connect_known_land_and_removal_disconnects_it() {
    let mut world = river_world(false, true);
    let (from, to) = (TileCoord::new(8, 8), TileCoord::new(24, 8));
    assert!(world.land_route(from, to).is_none());
    let mut bridge = building(10, ME, ConstructionState::Complete);
    bridge.kind = BuildingKind::Bridge;
    bridge.footprint = TileRect::from_xywh(15, 8, 3, 1);
    bridge.bridge_span = aoe_core::BridgeSpan::new(TileCoord::new(15, 8), TileCoord::new(17, 8));
    world.apply(M::Tick {
        revision: 1,
        tick: Tick(2),
        changed_units: vec![],
        removals: vec![],
        changed_players: vec![],
        changed_buildings: vec![bridge],
        building_removals: vec![],
    });
    assert_eq!(world.passable(TileCoord::new(16, 8)), Some(true));
    let route = world.land_route(from, to).unwrap();
    assert!(route.contains(&TileCoord::new(16, 8)));
    for edge in route.windows(2) {
        if edge.contains(&TileCoord::new(16, 8)) {
            assert_eq!(edge[0].y, edge[1].y, "no lateral entry on the deck");
        }
    }
    world.apply(M::Tick {
        revision: 1,
        tick: Tick(3),
        changed_units: vec![],
        removals: vec![],
        changed_players: vec![],
        changed_buildings: vec![],
        building_removals: vec![EntityId(10)],
    });
    assert!(world.land_route(from, to).is_none());
}

#[test]
fn bridge_site_heuristic_uses_only_explored_water_and_land_banks() {
    let from = TileCoord::new(15, 8);
    let to = TileCoord::new(24, 8);
    let world = river_world(false, true);
    assert_eq!(
        world.find_bridge_site(from, to),
        Some((from, TileCoord::new(17, 8)))
    );
    assert_eq!(
        river_kind_world(false, true, WaterKind::Shallow).find_bridge_site(from, to),
        Some((from, TileCoord::new(17, 8)))
    );
    assert_eq!(river_world(false, false).find_bridge_site(from, to), None);
    assert_eq!(
        world.find_bridge_site(TileCoord::new(i32::MAX, 0), to),
        None
    );
    assert_eq!(
        world.find_bridge_site(from, TileCoord::new(i32::MIN, 0)),
        None
    );
}

fn river_world(gap: bool, explored: bool) -> World {
    river_kind_world(gap, explored, WaterKind::River)
}

fn river_kind_world(gap: bool, explored: bool, water: WaterKind) -> World {
    let mut world = started();
    world.map_hash = Some([0; 32]);
    let generator = MapChunkGenerator::new([0; 32], 0, 200);
    let mut chunk = generator
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .unwrap();
    chunk.resources.clear();
    chunk.decorations.clear();
    for sample in &mut chunk.tiles {
        let river = sample.tile.x == 16 && !(gap && sample.tile.y == 27);
        sample.terrain.passable = !river;
        sample.terrain.game_height_level = 0;
        sample.terrain.surface.corner_game_height_levels = [0; 4];
        sample.terrain.surface.kind = SurfaceKind::Plateau;
        sample.terrain.water = if river { water } else { WaterKind::None };
    }
    Arc::make_mut(&mut world.catalog)
        .insert(&CompactChunk::encode(&chunk).unwrap())
        .unwrap();
    world.apply(M::FogState(FogState {
        revision: 1,
        tick: Tick(1),
        region: TileRect::from_xywh(0, 0, 32, 32),
        disabled: false,
        tiles: (0..1024)
            .map(|index| u8::from(explored || index % 32 < 16) * 2)
            .collect(),
        overview: Vec::new(),
    }));
    world
}

#[test]
fn a_one_tile_river_separates_known_land_components() {
    let world = river_world(false, true);
    assert!(
        world
            .land_route(TileCoord::new(8, 8), TileCoord::new(24, 8))
            .is_none()
    );
    assert!(
        world
            .land_route(TileCoord::new(8, 8), TileCoord::new(8, 24))
            .is_some()
    );
}

#[test]
fn a_known_ford_connects_the_banks_by_a_real_detour() {
    let world = river_world(true, true);
    let route = world
        .land_route(TileCoord::new(8, 8), TileCoord::new(24, 8))
        .unwrap();
    assert!(route.contains(&TileCoord::new(16, 27)));
    assert_eq!(route.first(), Some(&TileCoord::new(8, 8)));
    assert_eq!(route.last(), Some(&TileCoord::new(24, 8)));
}

#[test]
fn loaded_but_unexplored_ground_does_not_prove_connectivity() {
    let world = river_world(true, false);
    assert!(
        world
            .land_route(TileCoord::new(8, 8), TileCoord::new(24, 8))
            .is_none()
    );
    let next = world
        .land_frontier(TileCoord::new(8, 8), TileCoord::new(24, 8))
        .unwrap();
    assert!(next.x < 16, "stay on the explored bank: {next:?}");
}

#[test]
fn diagonal_steps_do_not_cut_a_river_corner() {
    let world = river_world(false, true);
    assert!(
        world
            .land_route(TileCoord::new(15, 8), TileCoord::new(17, 9))
            .is_none()
    );
}

#[test]
fn a_cliff_edge_between_open_tiles_is_not_a_land_connection() {
    let mut world = river_world(true, true);
    let generator = MapChunkGenerator::new([0; 32], 0, 200);
    let mut chunk = generator
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .unwrap();
    for sample in &mut chunk.tiles {
        sample.terrain.passable = true;
        sample.terrain.surface.kind = SurfaceKind::Plateau;
        sample.terrain.game_height_level = if sample.tile.x < 16 { 0 } else { 3 };
    }
    Arc::make_mut(&mut world.catalog)
        .insert(&CompactChunk::encode(&chunk).unwrap())
        .unwrap();
    assert!(
        world
            .land_route(TileCoord::new(8, 8), TileCoord::new(24, 8))
            .is_none()
    );
}

fn remembered_flat_land() -> World {
    let mut world = started();
    world.width = 16_000;
    world.height = 16_000;
    world.map_hash = Some([0; 32]);
    let generator = MapChunkGenerator::new([0; 32], 0, 200);
    let mut chunk = generator
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .unwrap();
    chunk.resources.clear();
    chunk.decorations.clear();
    for sample in &mut chunk.tiles {
        sample.terrain.passable = true;
        sample.terrain.game_height_level = 0;
        sample.terrain.surface.corner_game_height_levels = [0; 4];
        sample.terrain.surface.kind = SurfaceKind::Plateau;
        sample.terrain.water = WaterKind::None;
    }
    Arc::make_mut(&mut world.catalog)
        .insert(&CompactChunk::encode(&chunk).unwrap())
        .unwrap();
    world.apply(M::FogState(FogState {
        revision: 1,
        tick: Tick(1),
        region: TileRect::from_xywh(0, 0, 32, 32),
        disabled: false,
        tiles: (0..1024)
            .map(|index| {
                u8::from((8..=12).contains(&(index % 32)) && (8..=12).contains(&(index / 32))) * 2
            })
            .collect(),
        // At this map size, both (10, 10) and the unseen (31, 10) share pixel 0.
        overview: vec![(0, 2)],
    }));
    world.subscription = 2;
    world.apply(M::FogState(FogState {
        revision: 2,
        tick: Tick(2),
        region: TileRect::from_xywh(100, 100, 1, 1),
        disabled: false,
        tiles: vec![0],
        overview: Vec::new(),
    }));
    world
}

#[test]
fn remembered_overview_pixels_do_not_prove_a_land_route() {
    let world = remembered_flat_land();
    let unseen = TileCoord::new(31, 10);
    assert_eq!(world.visibility(unseen), Visibility::Explored);
    assert!(!world.fog().explored_exactly(unseen));
    assert!(world.land_route(TileCoord::new(10, 10), unseen).is_none());
}

#[test]
fn remembered_land_frontier_stops_at_exact_exploration() {
    let world = remembered_flat_land();
    let frontier = world
        .land_frontier(TileCoord::new(10, 10), TileCoord::new(31, 10))
        .unwrap();
    assert_eq!(frontier, TileCoord::new(12, 10));
    assert!(world.fog().explored_exactly(frontier));
}

#[test]
fn remembered_exact_exploration_still_proves_a_land_route() {
    let world = remembered_flat_land();
    let from = TileCoord::new(10, 10);
    let to = TileCoord::new(12, 10);
    let route = world.land_route(from, to).unwrap();
    assert_eq!(route.first(), Some(&from));
    assert_eq!(route.last(), Some(&to));
    assert!(route.iter().all(|tile| world.fog().explored_exactly(*tile)));
}
