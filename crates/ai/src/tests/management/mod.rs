//! The management managers (Steward, Vassal, General, expansion, castles),
//! the navy and the fortifications, called on a `Context` as the Python tests
//! do (`ai/tests/test_management.py`, `test_naval.py` and
//! `ai/tests/scaling/test_{kingdom,fortifications,threat_budget}.py`). Only the
//! tests Python plays through `Brain.decide` go through `Brain::decide`.
//!
//! The production surface these tests expect, named after Python's:
//! - `play::management::ManagementManager` (`Default`) with `step`, `expand`,
//!   `castles`, `recruit`, `settings`, `defenders` and `lead(ctx, &CombatManager)`,
//!   each `-> Result<(), OrderBudgetExhausted>`, and `save(ctx, kind, cost)`
//!   which sends no order; `funding_reserve` stays as it is.
//! - `play::naval::NavalManager` (`Default`) with `step`.
//! - `play::fortification::{FortificationManager, frontier}`: the manager
//!   (`Default`) with `step` and `lane(ctx, center, &rejected)`.
mod fortifications;
mod kingdom;
mod lanes;
mod naval;
mod threat_budget;
mod units;

use std::cell::RefCell;
use std::rc::Rc;

use aoe_sdk::{Age, BuildingKind, Event, Order, Tech, UnitKind};

use super::fake::{FakeWorld, base, building, economy, unit_of};
use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::context::Context;
use crate::seat::Seat;
use crate::view::{Building, Economy, Tile, Unit};

pub const CASTLE: [Tech; 3] = [Tech::Loom, Tech::FeudalAge, Tech::CastleAge];

/// A seat whose orders stay readable while a `Context` still borrows it
/// (Python reads `ctx.client` between manager calls).
#[derive(Clone, Default)]
pub struct Tape(Rc<RefCell<Vec<Order>>>);

impl Seat for Tape {
    fn send(&mut self, order: Order) -> u64 {
        let mut orders = self.0.borrow_mut();
        orders.push(order);
        orders.len() as u64
    }
}

impl Tape {
    pub fn orders(&self) -> Vec<Order> {
        self.0.borrow().clone()
    }
}

/// `client.named(...)` for the orders the management tests read.
pub trait Sent {
    fn builds(&self) -> Vec<(BuildingKind, Tile)>;
    fn trains(&self) -> Vec<(u32, UnitKind)>;
    fn garrisons(&self) -> Vec<(u32, u32)>;
    fn general_trains(&self) -> Vec<(u32, bool)>;
    fn general_mixes(&self) -> Vec<u32>;
    /// (unit, weights, cap, training)
    fn steward_settings(&self) -> Vec<(u32, [u8; 4], u8, bool)>;
    fn vassal_development(&self) -> Vec<(u32, bool)>;
    fn moves(&self) -> Vec<(u32, Tile)>;
    /// (members, destination, attack_move)
    fn formation_moves(&self) -> Vec<(Vec<u32>, Tile, bool)>;
    fn gathers(&self) -> Vec<(u32, u64)>;
    fn towers(&self) -> Vec<Tile> {
        self.builds()
            .into_iter()
            .filter(|(kind, _)| *kind == BuildingKind::WatchTower)
            .map(|(_, tile)| tile)
            .collect()
    }
}

impl Sent for [Order] {
    fn builds(&self) -> Vec<(BuildingKind, Tile)> {
        self.iter()
            .filter_map(|o| match o {
                Order::Build { kind, origin, .. } => Some((*kind, (origin.x, origin.y))),
                _ => None,
            })
            .collect()
    }
    fn trains(&self) -> Vec<(u32, UnitKind)> {
        self.iter()
            .filter_map(|o| match o {
                Order::Train { building, kind } => Some((building.0, *kind)),
                _ => None,
            })
            .collect()
    }
    fn garrisons(&self) -> Vec<(u32, u32)> {
        self.iter()
            .filter_map(|o| match o {
                Order::Garrison { unit, building } => Some((unit.0, building.0)),
                _ => None,
            })
            .collect()
    }
    fn general_trains(&self) -> Vec<(u32, bool)> {
        self.iter()
            .filter_map(|o| match o {
                Order::GeneralTrain { unit, enabled } => Some((unit.0, *enabled)),
                _ => None,
            })
            .collect()
    }
    fn general_mixes(&self) -> Vec<u32> {
        self.iter()
            .filter_map(|o| match o {
                Order::GeneralMix { unit, .. } => Some(unit.0),
                _ => None,
            })
            .collect()
    }
    fn steward_settings(&self) -> Vec<(u32, [u8; 4], u8, bool)> {
        self.iter()
            .filter_map(|o| match o {
                Order::StewardSettings {
                    unit,
                    weights,
                    cap,
                    training,
                } => Some((unit.0, *weights, *cap, *training)),
                _ => None,
            })
            .collect()
    }
    fn vassal_development(&self) -> Vec<(u32, bool)> {
        self.iter()
            .filter_map(|o| match o {
                Order::VassalDevelopment { unit, enabled } => Some((unit.0, *enabled)),
                _ => None,
            })
            .collect()
    }
    fn moves(&self) -> Vec<(u32, Tile)> {
        self.iter()
            .filter_map(|o| match o {
                Order::Move { unit, to } => Some((unit.0, (to.x, to.y))),
                _ => None,
            })
            .collect()
    }
    fn formation_moves(&self) -> Vec<(Vec<u32>, Tile, bool)> {
        self.iter()
            .filter_map(|o| match o {
                Order::FormationMove {
                    units,
                    to,
                    attack_move,
                    ..
                } => Some((
                    units.iter().map(|u| u.0).collect(),
                    (to.x, to.y),
                    *attack_move,
                )),
                _ => None,
            })
            .collect()
    }
    fn gathers(&self) -> Vec<(u32, u64)> {
        self.iter()
            .filter_map(|o| match o {
                Order::Gather { unit, resource } => Some((unit.0, *resource)),
                _ => None,
            })
            .collect()
    }
}

/// `test_imperial.castle_base`: the base, soldiers and the four Castle Age buildings.
pub fn castle_base(villagers: u32, soldiers: u32) -> (Vec<Unit>, Vec<Building>) {
    let (mut units, mut buildings) = base(villagers);
    units.extend((0..soldiers).map(|i| unit_of(300 + i, UnitKind::Spearman, (60, 60))));
    buildings.extend([
        building(600, BuildingKind::Barracks, (60, 50)),
        building(601, BuildingKind::ArcheryRange, (64, 50)),
        building(602, BuildingKind::Blacksmith, (68, 50)),
        building(603, BuildingKind::Stable, (72, 50)),
    ]);
    (units, buildings)
}

/// `test_imperial.castle_economy(**kwargs)` before the keyword changes.
pub fn castle_economy() -> Economy {
    Economy {
        food: 1500,
        wood: 900,
        gold: 1200,
        population: 70,
        population_cap: 120,
        age: Age::Castle,
        researched: CASTLE.to_vec(),
        ..economy()
    }
}

/// A Castle Age economy with the given stock and population.
pub fn castle_stock(
    (food, wood, gold, stone): (u32, u32, u32, u32),
    population: u32,
    cap: u32,
) -> Economy {
    Economy {
        food,
        wood,
        gold,
        stone,
        population,
        population_cap: cap,
        ..castle_economy()
    }
}

pub fn world_of(units: Vec<Unit>, buildings: Vec<Building>, economy: Economy) -> FakeWorld {
    let mut world = FakeWorld::new(units, buildings);
    world.economy = Some(economy);
    world
}

/// `test_kingdom.kingdom`: a Castle Age realm of `villagers` and `centers`
/// Town Centers 70 tiles apart, rich in everything, on a 1000-tile map.
pub struct Kingdom {
    pub world: FakeWorld,
    pub board: Blackboard,
    pub config: Config,
    tape: Tape,
}

pub fn kingdom(villagers: u32, centers: u32) -> Kingdom {
    let (units, mut buildings) = castle_base(villagers, 0);
    buildings.extend(
        (1..centers).map(|i| building(500 + i, BuildingKind::TownCenter, (50 + 70 * i as i32, 50))),
    );
    let stock = (10_000, 10_000, 10_000, 10_000);
    let mut world = world_of(units, buildings, castle_stock(stock, villagers + 1, 2000));
    world.size = (1000, 1000);
    Kingdom {
        world,
        board: Blackboard::default(),
        config: Config::default(),
        tape: Tape::default(),
    }
}

/// Any world with a fresh blackboard, default config and an empty tape.
pub fn scene(world: FakeWorld) -> Kingdom {
    Kingdom {
        world,
        board: Blackboard::default(),
        config: Config::default(),
        tape: Tape::default(),
    }
}

impl Kingdom {
    /// `Context(client, decision(world), board, config)`; read the orders
    /// through `tape()`.
    pub fn ctx(&mut self) -> Context<'_> {
        Context::new(
            &self.world,
            &[],
            &mut self.board,
            &self.config,
            &mut self.tape,
        )
    }

    /// `ctx.events = [...]`: a context that has seen `events`.
    pub fn ctx_with<'a>(&'a mut self, events: &'a [Event]) -> Context<'a> {
        Context::new(
            &self.world,
            events,
            &mut self.board,
            &self.config,
            &mut self.tape,
        )
    }

    pub fn tape(&self) -> Tape {
        self.tape.clone()
    }

    pub fn economy(&mut self) -> &mut Economy {
        self.world
            .economy
            .as_mut()
            .expect("the kingdom has an economy")
    }

    pub fn push_building(&mut self, building: Building) {
        self.world.buildings.push(building);
    }

    /// The tick of the world, as a number of ticks per `seconds`.
    pub fn ticks(&self, seconds: u64) -> u64 {
        seconds * u64::from(self.world.tick_hz)
    }
}

/// A building of ours with a Vassal or Steward seated inside.
pub fn seated(id: u32, kind: BuildingKind, tile: Tile, guest: (u32, UnitKind)) -> Building {
    Building {
        garrison: vec![(guest.0, guest.1, 50, 50)],
        ..building(id, kind, tile)
    }
}
