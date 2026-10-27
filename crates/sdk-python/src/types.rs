//! Read-only snapshots of what the bot sees, as plain Python objects.
use crate::enums::{
    Age, AnimalKind, AnimalTask, BuildingKind, CommandResult, ResourceKind, Tech, UnitKind,
    UnitTask,
};
use pyo3::prelude::*;

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct Unit {
    pub id: u32,
    pub owner: u16,
    pub mine: bool,
    pub kind: UnitKind,
    /// Tile coordinates.
    pub x: i32,
    pub y: i32,
    pub task: UnitTask,
    pub hp: u16,
    pub max_hp: u16,
    pub moving: bool,
    pub auto_scout: bool,
    pub cargo_count: u8,
    pub home_castle: Option<u32>,
    pub managed_by: Option<u32>,
    pub idle: bool,
    pub alive: bool,
    pub military: bool,
    pub carrying: Option<(ResourceKind, u16)>,
    pub gathering: Option<ResourceKind>,
    pub attack_target: Option<u32>,
    pub stance: crate::enums::CombatStance,
}

#[pymethods]
impl Unit {
    #[getter]
    fn tile(&self) -> (i32, i32) {
        (self.x, self.y)
    }
    #[getter]
    fn health(&self) -> f32 {
        f32::from(self.hp) / f32::from(self.max_hp.max(1))
    }
    fn __repr__(&self) -> String {
        format!(
            "Unit(id={}, {:?}, owner={}, tile=({}, {}), {:?}, hp={}/{})",
            self.id, self.kind, self.owner, self.x, self.y, self.task, self.hp, self.max_hp
        )
    }
}

impl From<aoe_sdk::Unit> for Unit {
    fn from(unit: aoe_sdk::Unit) -> Self {
        let tile = unit.tile();
        Self {
            id: unit.id().0,
            owner: unit.owner().0,
            mine: unit.mine,
            kind: unit.kind().into(),
            x: tile.x,
            y: tile.y,
            task: unit.state.task.into(),
            hp: unit.state.hp,
            max_hp: unit.state.max_hp,
            moving: unit.state.moving,
            auto_scout: unit.state.auto_scout,
            cargo_count: unit.state.cargo_count,
            home_castle: unit.state.home_castle.map(|id| id.0),
            managed_by: unit.state.managed_by.map(|id| id.0),
            stance: unit.state.stance.into(),
            idle: unit.idle(),
            alive: unit.alive(),
            military: unit.is_military(),
            carrying: unit.state.carried.map(|c| (c.kind.into(), c.amount)),
            gathering: unit.state.gathering.map(Into::into),
            attack_target: unit.state.attack_target.map(|id| id.0),
        }
    }
}

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct Building {
    pub id: u32,
    pub owner: u16,
    pub mine: bool,
    pub kind: BuildingKind,
    /// Top-left tile and size of the footprint.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub complete: bool,
    /// Construction progress in thousandths (1000 when complete).
    pub progress: u16,
    pub hp: u16,
    pub max_hp: u16,
    /// False for a building remembered from earlier, now out of sight.
    pub visible: bool,
    pub queue_length: usize,
    pub farm_reseeds: u8,
    pub garrison_capacity: u8,
    /// Occupants as (id, kind, hp, max_hp), visible to the owner only.
    pub garrison: Vec<(u32, UnitKind, u16, u16)>,
    pub rally_position: Option<(i32, i32)>,
    pub rally_resource: Option<u64>,
    pub rally_building: Option<u32>,
    pub rally_animal: Option<u32>,
    pub researching: Option<Tech>,
    pub idle: bool,
}

#[pymethods]
impl Building {
    #[getter]
    fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
    fn __repr__(&self) -> String {
        format!(
            "Building(id={}, {:?}, owner={}, tile=({}, {}), complete={})",
            self.id, self.kind, self.owner, self.x, self.y, self.complete
        )
    }
}

impl From<aoe_sdk::Building> for Building {
    fn from(building: aoe_sdk::Building) -> Self {
        let f = building.footprint();
        Self {
            id: building.id().0,
            owner: building.owner().0,
            mine: building.mine,
            kind: building.kind().into(),
            x: f.min.x,
            y: f.min.y,
            width: f.width(),
            height: f.height(),
            complete: building.complete(),
            progress: building.state.progress_permille,
            hp: building.state.hp,
            max_hp: building.state.max_hp,
            visible: building.visible,
            queue_length: building.state.training_queue.len(),
            farm_reseeds: building.state.farm_reseeds,
            garrison_capacity: building.kind().garrison_capacity(),
            garrison: building.state.garrison.iter().map(|u| (u.id.0, u.kind.into(), u.hp, u.max_hp)).collect(),
            rally_position: building.state.rally_point.map(|r| {
                let t = r.position.tile_floor();
                (t.x, t.y)
            }),
            rally_resource: building.state.rally_point.and_then(|r| match r.target {
                aoe_sdk::RallyTarget::Resource(id) => Some(id),
                _ => None,
            }),
            rally_building: building.state.rally_point.and_then(|r| match r.target {
                aoe_sdk::RallyTarget::Building(id) => Some(id.0),
                _ => None,
            }),
            rally_animal: building.state.rally_point.and_then(|r| match r.target {
                aoe_sdk::RallyTarget::Animal(id) => Some(id.0),
                _ => None,
            }),
            researching: building.state.research.map(|r| r.tech.into()),
            idle: building.idle(),
        }
    }
}

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct ResourceNode {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub kind: ResourceKind,
    pub remaining: u16,
}

#[pymethods]
impl ResourceNode {
    #[getter]
    fn tile(&self) -> (i32, i32) {
        (self.x, self.y)
    }
    fn __repr__(&self) -> String {
        format!(
            "ResourceNode(id={}, {:?}, tile=({}, {}), remaining={})",
            self.id, self.kind, self.x, self.y, self.remaining
        )
    }
}

impl From<aoe_sdk::ResourceNode> for ResourceNode {
    fn from(node: aoe_sdk::ResourceNode) -> Self {
        Self {
            id: node.id,
            x: node.tile.x,
            y: node.tile.y,
            kind: node.kind.into(),
            remaining: node.remaining,
        }
    }
}

#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct Animal {
    pub id: u32,
    pub kind: AnimalKind,
    pub x: i32,
    pub y: i32,
    pub hp: u16,
    pub task: AnimalTask,
    pub food_remaining: u16,
    pub owner: Option<u16>,
}

#[pymethods]
impl Animal {
    #[getter]
    fn tile(&self) -> (i32, i32) {
        (self.x, self.y)
    }
    fn __repr__(&self) -> String {
        format!(
            "Animal(id={}, {:?}, tile=({}, {}), food={})",
            self.id, self.kind, self.x, self.y, self.food_remaining
        )
    }
}

impl From<aoe_sdk::Animal> for Animal {
    fn from(animal: aoe_sdk::Animal) -> Self {
        let tile = animal.position.tile_floor();
        Self {
            id: animal.id.0,
            kind: animal.kind.into(),
            x: tile.x,
            y: tile.y,
            hp: animal.hp,
            task: animal.task.into(),
            food_remaining: animal.food_remaining,
            owner: animal.owner.map(|p| p.0),
        }
    }
}

/// The slot's own stockpile, population, age, research and market prices.
#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug)]
pub struct Economy {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
    pub population: u16,
    pub population_cap: u16,
    pub age: Age,
    pub researched: Vec<Tech>,
    /// Gold per 100 units at the Market.
    pub wood_price: u16,
    pub food_price: u16,
    pub stone_price: u16,
}

#[pymethods]
impl Economy {
    #[getter]
    fn free_population(&self) -> u16 {
        self.population_cap.saturating_sub(self.population)
    }
    fn has_researched(&self, tech: Tech) -> bool {
        self.researched.contains(&tech)
    }
    /// Whether the stockpile covers a cost given as food, wood, gold, stone.
    #[pyo3(signature = (food=0, wood=0, gold=0, stone=0))]
    fn can_afford(&self, food: u32, wood: u32, gold: u32, stone: u32) -> bool {
        self.food >= food && self.wood >= wood && self.gold >= gold && self.stone >= stone
    }
    fn __repr__(&self) -> String {
        format!(
            "Economy(food={}, wood={}, gold={}, stone={}, pop={}/{}, {:?})",
            self.food,
            self.wood,
            self.gold,
            self.stone,
            self.population,
            self.population_cap,
            self.age
        )
    }
}

impl From<aoe_sdk::Economy> for Economy {
    fn from(economy: aoe_sdk::Economy) -> Self {
        let s = economy.stockpile();
        let prices = economy.market_prices();
        Self {
            food: s.food,
            wood: s.wood,
            gold: s.gold,
            stone: s.stone,
            population: economy.population(),
            population_cap: economy.population_cap(),
            age: economy.age().into(),
            researched: aoe_sdk::Tech::ALL
                .into_iter()
                .filter(|tech| economy.has_researched(*tech))
                .map(Into::into)
                .collect(),
            wood_price: prices.wood,
            food_price: prices.food,
            stone_price: prices.stone,
        }
    }
}

/// Something that happened since the last decision. `kind` names it:
/// `unit_spawned`, `unit_died`, `building_completed`, `building_destroyed`,
/// `research_completed`, `age_advanced`, `under_attack`, `order_result`,
/// `server_error`, `world_reset`; the other fields are set as relevant.
#[pyclass(frozen, get_all, module = "aoeworld")]
#[derive(Clone, Debug, Default)]
pub struct Event {
    pub kind: String,
    pub id: Option<u32>,
    pub unit_kind: Option<UnitKind>,
    pub building_kind: Option<BuildingKind>,
    pub owner: Option<u16>,
    pub mine: Option<bool>,
    pub tech: Option<Tech>,
    pub age: Option<Age>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub attacker: Option<u32>,
    pub is_building: Option<bool>,
    pub sequence: Option<u64>,
    pub result: Option<CommandResult>,
    pub code: Option<u16>,
    pub message: Option<String>,
}

#[pymethods]
impl Event {
    fn __repr__(&self) -> String {
        format!(
            "Event({}, id={:?}, sequence={:?}, result={:?})",
            self.kind, self.id, self.sequence, self.result
        )
    }
}

impl From<&aoe_sdk::Event> for Event {
    fn from(event: &aoe_sdk::Event) -> Self {
        use aoe_sdk::Event as E;
        let base = |kind: &str| Event {
            kind: kind.to_owned(),
            ..Event::default()
        };
        match event {
            E::UnitSpawned { id, kind } => Event {
                id: Some(id.0),
                unit_kind: Some((*kind).into()),
                mine: Some(true),
                ..base("unit_spawned")
            },
            E::UnitDied {
                id,
                kind,
                owner,
                mine,
                tile,
            } => Event {
                id: Some(id.0),
                unit_kind: Some((*kind).into()),
                owner: Some(owner.0),
                mine: Some(*mine),
                x: Some(tile.x),
                y: Some(tile.y),
                ..base("unit_died")
            },
            E::BuildingCompleted { id, kind } => Event {
                id: Some(id.0),
                building_kind: Some((*kind).into()),
                mine: Some(true),
                ..base("building_completed")
            },
            E::BuildingDestroyed {
                id,
                kind,
                owner,
                mine,
            } => Event {
                id: Some(id.0),
                building_kind: Some((*kind).into()),
                owner: Some(owner.0),
                mine: Some(*mine),
                ..base("building_destroyed")
            },
            E::ResearchCompleted { tech } => Event {
                tech: Some((*tech).into()),
                ..base("research_completed")
            },
            E::AgeAdvanced { age } => Event {
                age: Some((*age).into()),
                ..base("age_advanced")
            },
            E::UnderAttack {
                target,
                attacker,
                building,
                tile,
            } => Event {
                id: Some(target.0),
                attacker: attacker.map(|id| id.0),
                is_building: Some(*building),
                x: Some(tile.x),
                y: Some(tile.y),
                mine: Some(true),
                ..base("under_attack")
            },
            E::OrderResult { sequence, result } => Event {
                sequence: Some(*sequence),
                result: Some((*result).into()),
                ..base("order_result")
            },
            E::ServerError { code, message } => Event {
                code: Some(*code),
                message: Some(message.clone()),
                ..base("server_error")
            },
            E::WorldReset => base("world_reset"),
        }
    }
}
