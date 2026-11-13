use super::*;
use aoe_map::{CompactChunk, GroundMaterial, MapChunkGenerator, SurfaceKind, WaterKind};
use std::sync::Arc;

fn shoreline(water: WaterKind, ford: bool, explored: bool) -> World {
    let mut world = started();
    world.map_hash = Some([0; 32]);
    let generator = MapChunkGenerator::new([0; 32], 0, 200);
    let mut chunk = generator
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .unwrap();
    chunk.resources.clear();
    chunk.decorations.clear();
    for sample in &mut chunk.tiles {
        let wet = sample.tile.x >= 16;
        sample.terrain.passable = !wet || ford;
        sample.terrain.game_height_level = 0;
        sample.terrain.surface.corner_game_height_levels = [0; 4];
        sample.terrain.surface.kind = SurfaceKind::Plateau;
        sample.terrain.water = if wet { water } else { WaterKind::None };
        sample.terrain.material = if wet && ford {
            GroundMaterial::Ford
        } else {
            GroundMaterial::TemperateGrass
        };
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
            .map(|i| u8::from(explored || i % 32 < 16) * 2)
            .collect(),
        overview: vec![(0, 2)],
    }));
    world
}

#[test]
fn dock_build_sites_accept_explored_river_lake_and_ocean_banks() {
    let bank = TileCoord::new(14, 10);
    for water in [
        WaterKind::River,
        WaterKind::Lake,
        WaterKind::Ocean,
        WaterKind::Shallow,
    ] {
        let world = shoreline(water, false, true);
        assert_eq!(
            world.find_build_site(BuildingKind::Dock, bank, 0, 1),
            Some(bank),
            "{water:?}"
        );
        assert!(
            world
                .find_build_site(BuildingKind::Barracks, bank, 0, 0)
                .is_none()
        );
    }
}

#[test]
fn dock_build_sites_reject_footprints_entirely_on_land_or_water() {
    let world = shoreline(WaterKind::River, false, true);
    for origin in [TileCoord::new(8, 10), TileCoord::new(20, 10)] {
        assert!(
            world
                .find_build_site(BuildingKind::Dock, origin, 0, 0)
                .is_none()
        );
    }
}

#[test]
fn dock_build_sites_reject_loaded_but_unexplored_banks() {
    let bank = TileCoord::new(14, 10);
    for (ford, explored) in [(false, false), (true, false)] {
        let world = shoreline(WaterKind::River, ford, explored);
        assert!(
            world
                .find_build_site(BuildingKind::Dock, bank, 0, 0)
                .is_none()
        );
    }
}

#[test]
fn dock_build_sites_accept_explored_wadeable_marsh_banks() {
    let bank = TileCoord::new(14, 10);
    let world = shoreline(WaterKind::River, true, true);
    assert_eq!(
        world.find_build_site(BuildingKind::Dock, bank, 0, 0),
        Some(bank)
    );
}

#[test]
fn water_queries_distinguish_known_water_from_land_and_unexplored_chunks() {
    let world = shoreline(WaterKind::River, false, true);
    assert_eq!(world.water_passable(TileCoord::new(20, 10)), Some(true));
    assert_eq!(world.water_passable(TileCoord::new(10, 10)), Some(false));
    assert_eq!(
        shoreline(WaterKind::River, false, false).water_passable(TileCoord::new(20, 10)),
        None
    );
    let from = TileCoord::new(20, 10);
    let toward = TileCoord::new(30, 20);
    let next = world.water_frontier(from, toward).unwrap();
    assert_eq!(next, toward);
    assert!(
        world
            .water_frontier(TileCoord::new(10, 10), toward)
            .is_none()
    );
    let coast = world.water_frontier(from, TileCoord::new(10, 10)).unwrap();
    assert_eq!(coast.x, 16, "a ship stays on its side of the shoreline");
}
