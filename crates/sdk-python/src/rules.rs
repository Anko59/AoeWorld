//! The game's public cost and time tables, for planning.
use crate::enums::{Age, BuildingKind, Tech, UnitKind};
use pyo3::prelude::*;

type Cost = (u32, u32, u32, u32);

fn cost(stock: aoe_sdk::Stockpile) -> Cost {
    (stock.food, stock.wood, stock.gold, stock.stone)
}

/// `(food, wood, gold, stone)` to place a building (per segment for walls).
#[pyfunction]
fn building_cost(kind: BuildingKind) -> Cost {
    cost(aoe_sdk::rules::building_cost(kind.into()))
}

/// Seconds one villager needs to build it.
#[pyfunction]
fn build_seconds(kind: BuildingKind) -> u32 {
    aoe_sdk::rules::building_build_seconds(kind.into())
}

/// `(width, height)` of the footprint in tiles.
#[pyfunction]
fn building_size(kind: BuildingKind) -> (i32, i32) {
    let kind: aoe_sdk::BuildingKind = kind.into();
    (kind.side(), kind.depth())
}

/// Population a completed building houses.
#[pyfunction]
fn building_population(kind: BuildingKind) -> u16 {
    aoe_sdk::rules::building_population(kind.into())
}

/// The completed building that must exist before this one can be built.
#[pyfunction]
fn building_requires(kind: BuildingKind) -> Option<BuildingKind> {
    aoe_sdk::rules::building_requires(kind.into()).map(Into::into)
}

/// Whether villagers may construct this kind at all.
#[pyfunction]
fn building_buildable(kind: BuildingKind) -> bool {
    aoe_sdk::rules::building_buildable(kind.into())
}

/// Earliest age in which villagers may construct this kind.
#[pyfunction]
fn building_required_age(kind: BuildingKind) -> Age {
    aoe_sdk::rules::building_required_age(kind.into()).into()
}

/// Units a completed building can train.
#[pyfunction]
fn trainable(building: BuildingKind) -> Vec<UnitKind> {
    aoe_sdk::rules::trainable(building.into())
        .into_iter()
        .map(Into::into)
        .collect()
}

/// `(food, wood, gold, stone)` to queue a unit.
#[pyfunction]
fn unit_cost(kind: UnitKind) -> Cost {
    cost(aoe_sdk::rules::unit_cost(kind.into()))
}

#[pyfunction]
fn train_seconds(kind: UnitKind) -> u32 {
    aoe_sdk::rules::unit_train_seconds(kind.into())
}

/// `(food, wood, gold, stone)` to research a technology.
#[pyfunction]
fn tech_cost(tech: Tech) -> Cost {
    cost(aoe_sdk::rules::tech_cost(tech.into()))
}

/// Resources a villager carries per trip.
#[pyfunction]
fn villager_capacity() -> u16 {
    aoe_sdk::rules::villager_capacity()
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Technology>()?;
    module.add_function(wrap_pyfunction!(technologies, module)?)?;
    module.add_function(wrap_pyfunction!(market_quote, module)?)?;
    module.add_function(wrap_pyfunction!(building_cost, module)?)?;
    module.add_function(wrap_pyfunction!(build_seconds, module)?)?;
    module.add_function(wrap_pyfunction!(building_size, module)?)?;
    module.add_function(wrap_pyfunction!(building_population, module)?)?;
    module.add_function(wrap_pyfunction!(building_requires, module)?)?;
    module.add_function(wrap_pyfunction!(building_buildable, module)?)?;
    module.add_function(wrap_pyfunction!(building_required_age, module)?)?;
    module.add_function(wrap_pyfunction!(trainable, module)?)?;
    module.add_function(wrap_pyfunction!(unit_cost, module)?)?;
    module.add_function(wrap_pyfunction!(train_seconds, module)?)?;
    module.add_function(wrap_pyfunction!(tech_cost, module)?)?;
    module.add_function(wrap_pyfunction!(villager_capacity, module)?)?;
    Ok(())
}

/// Current public technology tree, without a separate Python rules table.
#[pyclass(frozen, get_all)]
#[derive(Clone)]
struct Technology {
    tech: Tech,
    building: BuildingKind,
    age: Age,
    prerequisite: Option<Tech>,
    building_count: usize,
    building_kinds: Vec<BuildingKind>,
}

#[pyfunction]
fn technologies() -> Vec<Technology> {
    aoe_sdk::rules::technologies().into_iter().map(|row| Technology {
        tech: row.tech.into(), building: row.building.into(), age: row.age.into(),
        prerequisite: row.prerequisite.map(Into::into), building_count: row.building_count,
        building_kinds: row.building_kinds.into_iter().map(Into::into).collect(),
    }).collect()
}

#[pyfunction]
fn market_quote(price: u16, buy: bool) -> u32 {
    aoe_sdk::rules::market_quote(price, buy)
}
