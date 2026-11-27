//! What the AI reads of the world: plain snapshots ported field for field from
//! `crates/sdk-python/src/types.rs`, and the `View` trait every manager reads
//! the world through (the SDK's `World` implements it in `sdk_view`).
use aoe_sdk::{
    Age, AnimalKind, AnimalTask, BuildingKind, CombatStance, GeneralState, ResourceKind, Tech,
    UnitKind, UnitTask, Visibility,
};

/// A tile `(x, y)`.
pub type Tile = (i32, i32);

#[derive(Clone, Debug, PartialEq)]
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
    pub commanded_by: Option<u32>,
    pub army_size: Option<u16>,
    pub general_state: Option<GeneralState>,
    pub idle: bool,
    pub alive: bool,
    pub military: bool,
    pub carrying: Option<(ResourceKind, u16)>,
    pub gathering: Option<ResourceKind>,
    pub attack_target: Option<u32>,
    pub stance: CombatStance,
}

impl Unit {
    pub fn tile(&self) -> Tile {
        (self.x, self.y)
    }
    pub fn health(&self) -> f32 {
        f32::from(self.hp) / f32::from(self.max_hp.max(1))
    }
}

impl From<aoe_sdk::Unit> for Unit {
    fn from(unit: aoe_sdk::Unit) -> Self {
        let tile = unit.tile();
        Self {
            id: unit.id().0,
            owner: unit.owner().0,
            mine: unit.mine,
            kind: unit.kind(),
            x: tile.x,
            y: tile.y,
            task: unit.state.task,
            hp: unit.state.hp,
            max_hp: unit.state.max_hp,
            moving: unit.state.moving,
            auto_scout: unit.state.auto_scout,
            cargo_count: unit.state.cargo_count,
            home_castle: unit.state.home_castle.map(|id| id.0),
            managed_by: unit.state.managed_by.map(|id| id.0),
            commanded_by: unit.state.commanded_by.map(|id| id.0),
            army_size: unit.state.general.map(|g| g.army_size),
            general_state: unit.state.general.map(|g| g.state),
            stance: unit.state.stance,
            idle: unit.idle(),
            alive: unit.alive(),
            military: unit.is_military(),
            carrying: unit.state.carried.map(|c| (c.kind, c.amount)),
            gathering: unit.state.gathering,
            attack_target: unit.state.attack_target.map(|id| id.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    pub queued_units: Vec<UnitKind>,
    pub farm_reseeds: u8,
    pub garrison_capacity: u8,
    /// Occupants as (id, kind, hp, max_hp), visible to the owner only.
    pub garrison: Vec<(u32, UnitKind, u16, u16)>,
    pub rally_position: Option<Tile>,
    pub rally_resource: Option<u64>,
    pub rally_building: Option<u32>,
    pub rally_animal: Option<u32>,
    pub researching: Option<Tech>,
    pub idle: bool,
}

impl Building {
    pub fn center(&self) -> Tile {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

impl From<aoe_sdk::Building> for Building {
    fn from(building: aoe_sdk::Building) -> Self {
        let f = building.footprint();
        let rally = building.state.rally_point;
        Self {
            id: building.id().0,
            owner: building.owner().0,
            mine: building.mine,
            kind: building.kind(),
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
            queued_units: building
                .state
                .training_queue
                .iter()
                .map(|unit| unit.kind)
                .collect(),
            farm_reseeds: building.state.farm_reseeds,
            garrison_capacity: building.kind().garrison_capacity(),
            garrison: building
                .state
                .garrison
                .iter()
                .map(|u| (u.id.0, u.kind, u.hp, u.max_hp))
                .collect(),
            rally_position: rally.map(|r| {
                let t = r.position.tile_floor();
                (t.x, t.y)
            }),
            rally_resource: rally.and_then(|r| match r.target {
                aoe_sdk::RallyTarget::Resource(id) => Some(id),
                _ => None,
            }),
            rally_building: rally.and_then(|r| match r.target {
                aoe_sdk::RallyTarget::Building(id) => Some(id.0),
                _ => None,
            }),
            rally_animal: rally.and_then(|r| match r.target {
                aoe_sdk::RallyTarget::Animal(id) => Some(id.0),
                _ => None,
            }),
            researching: building.state.research.map(|r| r.tech),
            idle: building.idle(),
        }
    }
}

/// `ResourceNode` of the Python SDK.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub kind: ResourceKind,
    pub remaining: u16,
}

impl Node {
    pub fn tile(&self) -> Tile {
        (self.x, self.y)
    }
}

impl From<aoe_sdk::ResourceNode> for Node {
    fn from(node: aoe_sdk::ResourceNode) -> Self {
        Self {
            id: node.id,
            x: node.tile.x,
            y: node.tile.y,
            kind: node.kind,
            remaining: node.remaining,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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

impl Animal {
    pub fn tile(&self) -> Tile {
        (self.x, self.y)
    }
}

impl From<aoe_sdk::Animal> for Animal {
    fn from(animal: aoe_sdk::Animal) -> Self {
        let tile = animal.position.tile_floor();
        Self {
            id: animal.id.0,
            kind: animal.kind,
            x: tile.x,
            y: tile.y,
            hp: animal.hp,
            task: animal.task,
            food_remaining: animal.food_remaining,
            owner: animal.owner.map(|p| p.0),
        }
    }
}

/// The slot's own stockpile, population, age, research and market prices.
#[derive(Clone, Debug, PartialEq)]
pub struct Economy {
    pub food: u32,
    pub wood: u32,
    pub gold: u32,
    pub stone: u32,
    pub population: u32,
    pub population_cap: u32,
    pub age: Age,
    pub researched: Vec<Tech>,
    /// Gold per 100 units at the Market.
    pub wood_price: u16,
    pub food_price: u16,
    pub stone_price: u16,
}

impl Economy {
    pub fn free_population(&self) -> u32 {
        self.population_cap.saturating_sub(self.population)
    }
    pub fn has_researched(&self, tech: Tech) -> bool {
        self.researched.contains(&tech)
    }
    /// Whether the stockpile covers a cost given as food, wood, gold, stone.
    pub fn can_afford(&self, cost: [u32; 4]) -> bool {
        self.food >= cost[0]
            && self.wood >= cost[1]
            && self.gold >= cost[2]
            && self.stone >= cost[3]
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
            age: economy.age(),
            researched: Tech::ALL
                .into_iter()
                .filter(|tech| economy.has_researched(*tech))
                .collect(),
            wood_price: prices.wood,
            food_price: prices.food,
            stone_price: prices.stone,
        }
    }
}

/// The world as one decision reads it. Queries never block and never show more
/// than the slot's fog of war allows.
pub trait View {
    fn tick(&self) -> u64;
    fn tick_hz(&self) -> u32;
    fn map_size(&self) -> (i32, i32);
    fn me(&self) -> Option<u16>;
    fn economy(&self) -> Option<Economy>;
    fn my_units(&self) -> Vec<Unit>;
    fn my_buildings(&self) -> Vec<Building>;
    fn visible_enemies(&self) -> Vec<Unit>;
    fn known_enemy_buildings(&self) -> Vec<Building>;
    fn town_center(&self) -> Option<Building>;
    fn animals(&self) -> Vec<Animal>;
    fn relics(&self) -> Option<&aoe_sdk::RelicView>;
    fn resources(&self, kind: ResourceKind) -> Vec<Node>;
    fn visibility(&self, tile: Tile) -> Visibility;
    fn passable(&self, tile: Tile) -> Option<bool>;
    fn land_known(&self, tile: Tile) -> bool;
    fn land_route(&self, from: Tile, to: Tile) -> Option<Vec<Tile>>;
    fn land_frontier(&self, from: Tile, to: Tile) -> Option<Tile>;
    fn find_build_site(
        &self,
        kind: BuildingKind,
        near: Tile,
        max_radius: i32,
        gap: i32,
        avoid: &[Tile],
    ) -> Option<Tile>;
    fn find_bridge_site(&self, from: Tile, to: Tile) -> Option<(Tile, Tile)>;
}
