//! Bucketed wildlife lookups pick exactly what a scan of the world picks.
use super::super::nearby::Nearby;
use crate::gameplay::wildlife;
use aoe_core::{PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition};
use aoe_simulation::{AnimalKind, GameWorld, UnitKind};

#[test]
fn bucketed_animals_and_projectiles_match_a_full_scan() {
    let mut world = GameWorld::new(WorldConfig::new(512, 512, Seed(7)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let kinds = [AnimalKind::Sheep, AnimalKind::Deer, AnimalKind::Boar];
    for n in 0..300 {
        let _ = world.spawn_animal(
            kinds[n % 3],
            at(5 + (n as i32 * 37) % 500, 5 + (n as i32 * 91) % 500),
        );
    }
    for n in 0..12 {
        let (x, y) = (20 + n * 40, 30 + (n * 53) % 400);
        let archer = world
            .spawn_kind(PlayerId(0), UnitKind::Archer, at(x, y))
            .unwrap();
        let target = world.spawn_villager(PlayerId(1), at(x + 3, y)).unwrap();
        world.issue_attack(PlayerId(0), archer, target).unwrap();
    }
    let mut checked = (0, 0);
    for _ in 0..6 {
        world.advance();
        let nearby = Nearby::new(&world);
        let mut scratch = Vec::new();
        for (x, y, side) in [
            (0, 0, 512),
            (0, 0, 31),
            (17, 40, 64),
            (200, 100, 96),
            (480, 480, 32),
            (31, 31, 2),
        ] {
            let region = TileRect::from_xywh(x, y, side, side);
            for player in [PlayerId(0), PlayerId(1), PlayerId(2)] {
                let animals = wildlife::animals(&world, region, player);
                let near = nearby.animals(region, player, &mut scratch);
                assert_eq!(
                    wildlife::animals_from(&world, near, region, player),
                    animals
                );
                let shots = wildlife::projectiles(&world, region, player);
                let near = nearby.projectiles(region, &mut scratch);
                assert_eq!(
                    wildlife::projectiles_from(&world, near, region, player),
                    shots
                );
                checked.0 += animals.len();
                checked.1 += shots.len();
            }
        }
    }
    assert!(checked.0 > 300 && checked.1 > 0, "checked {checked:?}");
}
