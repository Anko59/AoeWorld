//! The economy managers through `Brain::decide`, ported from `ai/tests`
//! (`test_logic.py`, `test_worker_recovery.py`, `test_imperial.py`). Every test
//! plays one decision against a fake world and reads the orders it sent.
mod imperial;
mod logic;
mod recovery;
mod upkeep;

use aoe_sdk::{Age, AnimalKind, AnimalTask, BuildingKind, Event, Order, Tech, UnitKind};

use super::fake::{FakeWorld, Recorder, economy};
use crate::brain::Brain;
use crate::config::Config;
use crate::view::{Animal, Economy, Tile};

/// `test_logic.play`: one decision of `brain` against `world`.
pub fn play(world: &FakeWorld, brain: &mut Brain, events: &[Event]) -> Recorder {
    let mut seat = Recorder::default();
    brain.decide(world, events, &mut seat);
    seat
}

/// `play` with a fresh default brain.
pub fn play_fresh(world: &FakeWorld) -> (Brain, Recorder) {
    let mut brain = Brain::new(Config::default());
    let seat = play(world, &mut brain, &[]);
    (brain, seat)
}

/// `fakes.economy(**kwargs)` with the usual knobs.
pub fn eco(food: u32, wood: u32, gold: u32, population: u32, cap: u32) -> Economy {
    Economy {
        food,
        wood,
        gold,
        population,
        population_cap: cap,
        ..economy()
    }
}

pub fn with_age(economy: Economy, age: Age, researched: &[Tech]) -> Economy {
    Economy {
        age,
        researched: researched.to_vec(),
        ..economy
    }
}

pub fn animal(id: u32, tile: Tile, kind: AnimalKind) -> Animal {
    Animal {
        id,
        kind,
        x: tile.0,
        y: tile.1,
        hp: 7,
        task: AnimalTask::Idle,
        food_remaining: 100,
        owner: None,
    }
}

/// `(building, kind)` of every `Train` order, in the order sent.
pub fn trains(seat: &Recorder) -> Vec<(u32, UnitKind)> {
    seat.orders
        .iter()
        .filter_map(|o| match o {
            Order::Train { building, kind } => Some((building.0, *kind)),
            _ => None,
        })
        .collect()
}

/// `(building, tech)` of every `Research` order, in the order sent.
pub fn researches(seat: &Recorder) -> Vec<(u32, Tech)> {
    seat.orders
        .iter()
        .filter_map(|o| match o {
            Order::Research { building, tech } => Some((building.0, *tech)),
            _ => None,
        })
        .collect()
}

/// `(kind, origin)` of every `Build` order, in the order sent.
pub fn builds(seat: &Recorder) -> Vec<(BuildingKind, Tile)> {
    seat.orders
        .iter()
        .filter_map(|o| match o {
            Order::Build { kind, origin, .. } => Some((*kind, (origin.x, origin.y))),
            _ => None,
        })
        .collect()
}

pub fn built_kinds(seat: &Recorder) -> Vec<BuildingKind> {
    builds(seat).into_iter().map(|(kind, _)| kind).collect()
}

/// `(unit, resource)` of every `Gather` order.
pub fn gathers(seat: &Recorder) -> Vec<(u32, u64)> {
    seat.orders
        .iter()
        .filter_map(|o| match o {
            Order::Gather { unit, resource } => Some((unit.0, *resource)),
            _ => None,
        })
        .collect()
}

/// `(farmer, farm)` of every `Farm` order.
pub fn farms(seat: &Recorder) -> Vec<(u32, u32)> {
    seat.orders
        .iter()
        .filter_map(|o| match o {
            Order::Farm { unit, farm } => Some((unit.0, farm.0)),
            _ => None,
        })
        .collect()
}

/// The quarry of every `Hunt` order.
pub fn hunts(seat: &Recorder) -> Vec<u32> {
    seat.orders
        .iter()
        .filter_map(|o| match o {
            Order::Hunt { animal, .. } => Some(animal.0),
            _ => None,
        })
        .collect()
}

/// The 1-based sequence the recorder gave the first order matching `pick`.
pub fn sequence_of(seat: &Recorder, pick: impl Fn(&Order) -> bool) -> u64 {
    seat.orders
        .iter()
        .position(pick)
        .map(|i| i as u64 + 1)
        .expect("the order was sent")
}
