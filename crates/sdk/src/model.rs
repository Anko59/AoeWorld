//! Typed views of what the server tells a slot. These wrap the wire records
//! rather than copy them, so nothing the browser sees is lost.
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
pub use aoe_protocol::{
    Age, AnimalKind, AnimalState as Animal, AnimalTask, BuildingKind, CarriedResource,
    CommandResult, ConstructionState, GameplayBuildingState, GameplayPlayerState,
    GameplayUnitState, GarrisonUnitState, MarketPrices, ProjectileState as Projectile, RallyPoint,
    RallyTarget, ResourceKind, Stockpile, Tech, TradeResource, UnitKind, UnitTask,
};

/// A unit the slot can currently see (its own, or a visible foreign one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    pub state: GameplayUnitState,
    pub mine: bool,
}

impl Unit {
    pub fn id(&self) -> EntityId {
        self.state.id
    }
    pub fn owner(&self) -> PlayerId {
        self.state.player
    }
    pub fn kind(&self) -> UnitKind {
        self.state.kind
    }
    pub fn position(&self) -> WorldPosition {
        self.state.position
    }
    pub fn tile(&self) -> TileCoord {
        self.state.position.tile_floor()
    }
    pub fn cargo_count(&self) -> u8 {
        self.state.cargo_count
    }
    pub fn idle(&self) -> bool {
        self.state.task == UnitTask::Idle && !self.state.moving
    }
    /// Dying or dead units stay listed for about a second; they are not targets.
    pub fn alive(&self) -> bool {
        self.state.hp > 0 && self.state.task != UnitTask::Dying && self.state.death_tick.is_none()
    }
    pub fn is_military(&self) -> bool {
        !matches!(
            self.state.kind,
            UnitKind::Villager | UnitKind::Scout | UnitKind::FishingShip | UnitKind::TransportShip
        )
    }
    /// Hit points as a fraction of the maximum, in `0.0..=1.0`.
    pub fn health(&self) -> f32 {
        f32::from(self.state.hp) / f32::from(self.state.max_hp.max(1))
    }
}

/// A building the slot owns or remembers. `visible` is false for a building
/// last observed earlier and now fogged: its state may be out of date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Building {
    pub state: GameplayBuildingState,
    pub mine: bool,
    pub visible: bool,
}

impl Building {
    pub fn garrison(&self) -> &[GarrisonUnitState] {
        &self.state.garrison
    }
    pub fn garrison_capacity(&self) -> u8 {
        self.kind().garrison_capacity()
    }
    pub fn id(&self) -> EntityId {
        self.state.id
    }
    pub fn kind(&self) -> BuildingKind {
        self.state.kind
    }
    pub fn owner(&self) -> PlayerId {
        self.state.player
    }
    pub fn complete(&self) -> bool {
        self.state.construction == ConstructionState::Complete
    }
    pub fn footprint(&self) -> TileRect {
        self.state.footprint
    }
    /// The tile in the middle of the footprint.
    pub fn center(&self) -> TileCoord {
        let f = self.state.footprint;
        TileCoord::new((f.min.x + f.max.x) / 2, (f.min.y + f.max.y) / 2)
    }
    pub fn idle(&self) -> bool {
        self.complete() && self.state.training_queue.is_empty() && self.state.research.is_none()
    }
}

/// The slot's own economy: stockpile, population, age and research.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Economy {
    pub state: GameplayPlayerState,
}

impl Economy {
    pub fn stockpile(&self) -> Stockpile {
        self.state.stockpile
    }
    pub fn age(&self) -> Age {
        self.state.age
    }
    pub fn population(&self) -> u16 {
        self.state.population_used
    }
    pub fn population_cap(&self) -> u16 {
        self.state.population_cap
    }
    pub fn free_population(&self) -> u16 {
        self.state
            .population_cap
            .saturating_sub(self.state.population_used)
    }
    pub fn has_researched(&self, tech: Tech) -> bool {
        self.state.researched & tech.bit() != 0
    }
    pub fn market_prices(&self) -> MarketPrices {
        self.state.market_prices
    }
    pub fn can_afford(&self, cost: Stockpile) -> bool {
        let s = self.state.stockpile;
        s.food >= cost.food && s.wood >= cost.wood && s.gold >= cost.gold && s.stone >= cost.stone
    }
}

/// A gatherable node on the map, known to the slot because its tile has been
/// explored. `remaining` is the last amount the slot saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceNode {
    pub id: u64,
    pub tile: TileCoord,
    pub kind: ResourceKind,
    pub remaining: u16,
}

/// How a slot sees one tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Visibility {
    Unexplored,
    Explored,
    Visible,
}

/// Squared tile distance, for nearest-first searches.
pub fn distance_squared(a: TileCoord, b: TileCoord) -> i64 {
    let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
    dx * dx + dy * dy
}
