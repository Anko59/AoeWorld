//! Work-start and ongoing authoritative facing, independent of the arrival route.
use super::*;
use crate::{AnimalKind, BuildingKind, Facing};

const DIRECTIONS: [(i32, i32, Facing); 8] = [
    (1, 1, Facing::South),
    (1, 0, Facing::SouthEast),
    (1, -1, Facing::East),
    (0, -1, Facing::NorthEast),
    (-1, -1, Facing::North),
    (-1, 0, Facing::NorthWest),
    (-1, 1, Facing::West),
    (0, 1, Facing::SouthWest),
];

fn at(x: i32, y: i32) -> WorldPosition {
    position(TileCoord::new(x, y)).unwrap()
}

fn flat() -> GameWorld {
    GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap()
}

fn assert_work(world: &mut GameWorld, id: EntityId, task: UnitTask, expected: Facing) {
    // Poison the last walking direction at start and again during work. Every
    // tick must restore facing; a one-time order/arrival fix is insufficient.
    for phase in 0..2 {
        let index = world.lookup[&id];
        world.units[index].state.facing = if expected == Facing::North {
            Facing::South
        } else {
            Facing::North
        };
        let changed = world.advance();
        let unit = world.unit(id).unwrap();
        assert_eq!(unit.task, task, "phase {phase}");
        assert_eq!(unit.facing, expected, "{task:?}, phase {phase}");
        assert!(
            changed.iter().any(|u| u.id == id),
            "facing must reach subscribers"
        );
    }
}

#[test]
fn each_resource_work_kind_faces_its_target_in_all_eight_directions() {
    for kind in [
        ResourceKind::Wood,
        ResourceKind::Gold,
        ResourceKind::Stone,
        ResourceKind::Food,
    ] {
        let (mut world, old) = resource_mainland();
        let Terrain::Map { generator, overlay } = &mut world.terrain else {
            unreachable!()
        };
        overlay
            .deplete(generator, old.id, old.initial_amount)
            .unwrap();
        let node = (-4..=4)
            .flat_map(|dy| (-4..=4).map(move |dx| TileCoord::new(old.tile.x + dx, old.tile.y + dy)))
            .filter_map(|tile| ResourceNode::starting(tile, kind))
            .find(|node| overlay.insert_starting_resource(generator, *node).is_ok())
            .expect("an unoccupied native tile for each starting resource");
        for (dx, dy, expected) in DIRECTIONS {
            let worker = world
                .spawn_villager(PlayerId(0), at(node.tile.x - dx, node.tile.y - dy))
                .unwrap();
            world.issue_gather(PlayerId(0), worker, node.id).unwrap();
            assert_work(&mut world, worker, UnitTask::Gathering, expected);
            world.remove_unit(worker);
        }
    }
}

#[test]
fn builders_and_building_attackers_face_the_nearest_footprint_tile() {
    // Off-centre side positions distinguish the nearest tile from the centre.
    for (x, y, expected) in [
        (99, 99, Facing::South),
        (99, 100, Facing::SouthEast),
        (99, 104, Facing::East),
        (100, 104, Facing::NorthEast),
        (104, 104, Facing::North),
        (104, 103, Facing::NorthWest),
        (104, 99, Facing::West),
        (103, 99, Facing::SouthWest),
    ] {
        let mut world = flat();
        let rect = TileRect::new(TileCoord::new(100, 100), TileCoord::new(104, 104));
        let building = world
            .insert_building(PlayerId(0), BuildingKind::TownCenter, rect, false)
            .unwrap();
        let worker = world.spawn_villager(PlayerId(0), at(x, y)).unwrap();
        world
            .issue_construct(PlayerId(0), &[worker], building)
            .unwrap();
        assert_work(&mut world, worker, UnitTask::Building, expected);
        world.remove_unit(worker);
        let attacker = world.spawn_militia(PlayerId(1), at(x, y)).unwrap();
        world.issue_attack(PlayerId(1), attacker, building).unwrap();
        assert_work(&mut world, attacker, UnitTask::Attacking, expected);
    }
}

#[test]
fn farmers_face_the_farm_from_every_surrounding_tile() {
    for (dx, dy, expected) in DIRECTIONS {
        let mut world = flat();
        let rect = TileRect::new(TileCoord::new(100, 100), TileCoord::new(103, 103));
        let farm = world
            .insert_building(PlayerId(0), BuildingKind::Farm, rect, true)
            .unwrap();
        let worker = world
            .spawn_villager(PlayerId(0), at(101 - dx, 101 - dy))
            .unwrap();
        world.issue_farm(PlayerId(0), worker, farm).unwrap();
        // Farm approaches ordinarily arrive on one of these eight cells.
        let index = world.lookup[&worker];
        world.stop_movement(index);
        assert_work(&mut world, worker, UnitTask::Farming, expected);
    }
}

#[test]
fn hunters_and_carvers_face_the_animal_in_all_eight_directions() {
    for kind in [AnimalKind::Sheep, AnimalKind::Boar] {
        for (dx, dy, expected) in DIRECTIONS {
            let mut world = flat();
            let worker = world
                .spawn_villager(PlayerId(0), at(100 - dx, 100 - dy))
                .unwrap();
            let animal = world.spawn_animal(kind, at(100, 100)).unwrap();
            // For diagonal sheep the one-tile strike reach needs a closer
            // fractional position; preserve the same direction vector.
            let index = world.lookup[&worker];
            world.units[index].state.position =
                WorldPosition::new(at(100, 100).x - dx * 512, at(100, 100).y - dy * 512);
            world.issue_hunt(PlayerId(0), &[worker], animal).unwrap();
            let task = if kind == AnimalKind::Sheep {
                UnitTask::Shepherding
            } else {
                UnitTask::Hunting
            };
            assert_work(&mut world, worker, task, expected);
            world.strike_animal(animal, worker, 255);
            for _ in 0..world.config.tick_hz {
                world.advance();
            }
            world.begin_carcass_gather(worker, animal).unwrap();
            // Exercise carving from each approach tile, independent of which
            // equally near tile the route planner would select.
            world.stop_movement(index);
            world.units[index].state.position = at(100 - dx, 100 - dy);
            assert_work(&mut world, worker, task, expected);
        }
    }
}

#[test]
fn melee_attackers_face_unit_targets_in_all_eight_directions() {
    for (dx, dy, expected) in DIRECTIONS {
        let mut world = flat();
        let attacker = world
            .spawn_militia(PlayerId(0), at(100 - dx, 100 - dy))
            .unwrap();
        let target = world.spawn_militia(PlayerId(1), at(100, 100)).unwrap();
        let index = world.lookup[&attacker];
        world.units[index].state.position =
            WorldPosition::new(at(100, 100).x - dx * 512, at(100, 100).y - dy * 512);
        world.issue_attack(PlayerId(0), attacker, target).unwrap();
        assert_work(&mut world, attacker, UnitTask::Attacking, expected);
    }
}
