//! A long, busy match: ten villages whose units gather, hunt, herd, train,
//! fight, die, stop and are sent across the map, again and again. Every tick
//! of `advance` checks (under `cfg(test)`) that the route scheduler's stored
//! counters equal a recount from the units, so a planner leaked by any of
//! these paths fails here on the tick it happens.
use super::*;
use crate::{Stockpile, UnitKind};

const PLAYERS: u16 = 10;
const SPACING: i32 = 160;

fn busy_world() -> GameWorld {
    let config = WorldConfig::new(2_048, 2_048, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = super::super::far_orders::prepared_terrain(vec![0; 4], vec![28; 4]);
    for slot in 0..PLAYERS {
        let start = TileCoord::new(
            300 + i32::from(slot % 5) * SPACING,
            300 + i32::from(slot / 5) * SPACING,
        );
        world
            .initialize_village(PlayerId(slot), start)
            .expect("village");
    }
    world
}

/// A small deterministic generator, so a failure replays exactly.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

fn tile_position(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x.clamp(1, 2_046), y.clamp(1, 2_046)))
        .expect("tile")
}

/// One random order to one random living unit.
fn random_order(world: &mut GameWorld, rng: &mut Lcg) {
    let units = world.units().filter(|u| u.hp > 0).collect::<Vec<_>>();
    if units.is_empty() {
        return;
    }
    let unit = units[rng.next(units.len())];
    let (player, id) = (unit.player, unit.id);
    let here = unit.position.tile_floor();
    match rng.next(9) {
        // Far and near moves: long multi-leg routes hold continuations.
        0 | 1 => {
            let reach = if rng.next(2) == 0 { 600 } else { 30 };
            let x = here.x + rng.next(2 * reach) as i32 - reach as i32;
            let y = here.y + rng.next(2 * reach) as i32 - reach as i32;
            let _ = world.issue_move(id, tile_position(x, y));
        }
        2 => {
            let nodes = world.starting_resources();
            if !nodes.is_empty() {
                let node = nodes[rng.next(nodes.len())];
                let _ = world.issue_gather(player, id, node.id);
            }
        }
        3 => {
            let enemies = units
                .iter()
                .filter(|u| u.player != player)
                .collect::<Vec<_>>();
            if !enemies.is_empty() {
                let target = enemies[rng.next(enemies.len())].id;
                let _ = world.issue_attack(player, id, target);
            }
        }
        4 => {
            let animals = world
                .animals()
                .filter(|a| a.hp > 0)
                .map(|a| (a.id, a.kind))
                .collect::<Vec<_>>();
            if !animals.is_empty() {
                let (animal, kind) = animals[rng.next(animals.len())];
                if kind == AnimalKind::Sheep {
                    let x = here.x + rng.next(80) as i32 - 40;
                    let y = here.y + rng.next(80) as i32 - 40;
                    let _ = world.issue_herd(player, animal, tile_position(x, y));
                } else {
                    let _ = world.issue_hunt(player, &[id], animal);
                }
            }
        }
        5 => {
            let _ = world.issue_stop(player, id);
        }
        6 => {
            let centers = world
                .buildings()
                .filter(|b| b.player == player && b.kind == BuildingKind::TownCenter)
                .map(|b| b.id)
                .collect::<Vec<_>>();
            for center in centers {
                let _ = world.issue_train(player, center, UnitKind::Villager);
            }
        }
        // A group order of one player's units, as a person sends an army.
        _ => {
            let x = here.x + rng.next(400) as i32 - 200;
            let y = here.y + rng.next(400) as i32 - 200;
            for own in units.iter().filter(|u| u.player == player).take(40) {
                let _ = world.issue_move(own.id, tile_position(x, y));
            }
        }
    }
}

#[test]
fn route_counters_stay_exact_through_a_long_busy_match() {
    let mut world = busy_world();
    for slot in 0..PLAYERS {
        if let Some(economy) = world.economy.players.get_mut(&PlayerId(slot)) {
            economy.stockpile = Stockpile {
                food: 60_000,
                wood: 60_000,
                gold: 60_000,
                stone: 60_000,
            };
        }
    }
    let mut rng = Lcg(0x5eed);
    // 4,000 ticks (over three game minutes at 20 Hz) with six orders a tick:
    // far denser in orders than a 30-minute human game.
    let mut peak_planners = 0;
    for _ in 0..4_000 {
        for _ in 0..6 {
            random_order(&mut world, &mut rng);
        }
        world.advance();
        peak_planners = peak_planners.max(world.active_planner_count);
        // `advance` itself panics on drift; this keeps the check explicit.
        assert_eq!(world.route_accounting_error(), None);
    }
    assert!(peak_planners > 0, "the run never held a route planner");
    assert!(
        world.units().filter(|u| u.hp == 0).count() > 0
            || world.units().count() > usize::from(PLAYERS) * 4,
        "the run neither killed nor trained a unit"
    );
}
