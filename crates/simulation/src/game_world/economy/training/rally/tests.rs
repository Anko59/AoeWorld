use super::*;
use crate::AnimalKind;
use aoe_core::{Seed, TileCoord, WorldConfig};

#[test]
fn exhausted_animal_rallies_keep_the_food_profession_after_the_carcass_is_removed() {
    for kind in [AnimalKind::Sheep, AnimalKind::Deer] {
        let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
        world.set_fog_disabled(true);
        let center = world
            .place_town_center(PlayerId(0), TileCoord::new(90, 90))
            .unwrap();
        world
            .economy
            .players
            .get_mut(&PlayerId(0))
            .unwrap()
            .stockpile
            .food = 200;
        let mut workers = Vec::new();
        for x in [99, 105] {
            workers.push(
                world
                    .spawn_villager(PlayerId(0), position(TileCoord::new(x, 100)).unwrap())
                    .unwrap(),
            );
        }
        let decoy = if kind == AnimalKind::Sheep {
            AnimalKind::Deer
        } else {
            AnimalKind::Sheep
        };
        let mut animals = Vec::new();
        for (x, animal_kind) in [(100, kind), (104, kind), (102, decoy)] {
            animals.push(
                world
                    .spawn_animal(animal_kind, position(TileCoord::new(x, 100)).unwrap())
                    .unwrap(),
            );
        }
        for _ in 0..20 {
            world.advance();
        }
        for id in &animals {
            world.strike_animal(*id, workers[0], 255);
        }
        for _ in 0..20 {
            world.advance();
        }
        world
            .issue_set_rally(PlayerId(0), center, RallyTarget::Animal(animals[0]))
            .unwrap();
        while world.deplete_carcass(animals[0]).is_some() {}
        for _ in 0..world.config.tick_hz * (crate::CORPSE_SECONDS + crate::DYING_SECONDS + 2) {
            world.advance();
        }
        assert!(world.animal(animals[0]).is_none());
        world
            .issue_train(PlayerId(0), center, UnitKind::Villager)
            .unwrap();
        for _ in 0..1000 {
            world.advance();
        }
        let trained = world.units().max_by_key(|u| u.id).unwrap().id;
        assert_eq!(
            world.economy.gatherers.get(&trained).and_then(|t| t.animal),
            Some(animals[1]),
            "{kind:?}: a nearer different food profession must not win"
        );
    }
}
