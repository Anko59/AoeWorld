//! Small prepared-coast regressions for water movement, combat and cargo.
use super::*;
use crate::{EconomyError, UnitKind, UnitTask};

mod combat;
mod crossing;
mod healing;
mod journeys;
mod movement;
mod research;
mod stats;
mod transport;

fn at(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).unwrap()
}

fn advance(world: &mut GameWorld, ticks: u32) {
    for _ in 0..ticks {
        world.advance();
    }
}

pub(super) fn coast() -> GameWorld {
    coast_coverage(100)
}

fn coast_coverage(coverage: u8) -> GameWorld {
    let (mut package, pages, mut water) = economy::fixture::flat_fixture(true);
    package.request.requested_side_meters = 15_360;
    water[0].ocean_coverage_percent = vec![0, coverage, 0, coverage];
    water[1].ocean_coverage_percent = vec![coverage / 2];
    for (level, page) in package
        .environment
        .water
        .as_mut()
        .unwrap()
        .levels
        .iter_mut()
        .zip(&water)
    {
        level.ordered_page_root =
            aoe_map::ordered_water_page_root(std::slice::from_ref(page)).unwrap();
    }
    let package = MapPackage::with_prepared_environment(
        1,
        package.request,
        Vec::new(),
        package.projection,
        package.provenance,
        package.environment,
    )
    .unwrap();
    GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap()
}

/// An unobstructed east-facing bank with room for ships on its water side.
fn bank(world: &GameWorld) -> (TileCoord, TileCoord) {
    for y in 20..100 {
        for x in 20..world.config().width_tiles - 16 {
            let land = TileCoord::new(x, y);
            let water = TileCoord::new(x + 1, y);
            if world
                .unit_tile_passable(UnitKind::Villager, land, &|| false)
                .unwrap()
                && (0..12).all(|dx| {
                    world
                        .unit_tile_passable(
                            UnitKind::FishingShip,
                            TileCoord::new(water.x + dx, water.y),
                            &|| false,
                        )
                        .unwrap()
                })
            {
                return (land, water);
            }
        }
    }
    panic!("prepared coast needs adjacent land and twelve clear water tiles");
}

fn ship(world: &mut GameWorld, kind: UnitKind, tile: TileCoord, owner: u16) -> EntityId {
    world.spawn_kind(PlayerId(owner), kind, at(tile)).unwrap()
}

#[test]
fn land_formations_reject_ships_atomically() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let a = ship(&mut world, UnitKind::Galley, water, 0);
    let b = ship(
        &mut world,
        UnitKind::Galley,
        TileCoord::new(water.x + 2, water.y),
        0,
    );
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_formation_move(
            PlayerId(0),
            &[a, b],
            aoe_core::Formation::Line,
            at(water),
            false,
            false
        ),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(before, world.canonical_hash());
}

mod river;
mod traffic;
