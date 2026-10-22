//! Integer wildlife state, local AI and villager hunting jobs.
use super::{EconomyError, GameUnit, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod ai;
mod carcass;
mod hash;
mod hunting;
mod spawning;
mod table;
pub use table::{AnimalKind, WILDLIFE_RULES, WildlifeRules};

pub const MAX_ANIMALS: usize = 4096;
pub const MAX_HUNTERS: usize = 2_048;
const CELL_TILES: i32 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AnimalTask {
    Idle,
    Fleeing,
    Attacking,
    Dead,
    Carcass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GameAnimal {
    pub id: EntityId,
    pub kind: AnimalKind,
    pub owner: Option<PlayerId>,
    pub hp: u16,
    pub position: WorldPosition,
    pub task: AnimalTask,
    pub food_remaining: u16,
    pub target: Option<EntityId>,
    pub facing: crate::Facing,
    pub moving: bool,
    pub death_tick: Option<aoe_core::Tick>,
}

#[derive(Clone, Copy, Debug)]
struct StoredAnimal {
    state: GameAnimal,
    home: WorldPosition,
    cooldown: u32,
    speed_carry: u64,
    decay_carry: u64,
    herd: Option<WorldPosition>,
}

#[derive(Clone, Copy, Debug)]
struct HuntJob {
    animal: EntityId,
    repath_tick: u64,
}

/// A live unit mirrored into the wildlife cell grid, so animals can find
/// nearby units through point lookups instead of scanning the population.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CellUnit {
    id: EntityId,
    position: WorldPosition,
    player: PlayerId,
    kind: UnitKind,
    alive: bool,
}

/// Actual visits by the last tick, including broad-phase pairs and due bodies.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WildlifeStats {
    pub cells: u32,
    pub candidate_pairs: u32,
    pub animal_steps: u32,
    pub hunters: u32,
    pub carcasses: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Wildlife {
    animals: BTreeMap<EntityId, StoredAnimal>,
    cells: BTreeMap<(i32, i32), BTreeSet<EntityId>>,
    /// Chasing/fleeing animals get a cleanup step if nearby units disappear.
    engaged: BTreeSet<EntityId>,
    /// Units mirrored by wildlife cell. Only point lookups and per-cell
    /// edits are made, so the hash map never biases a scan order.
    unit_cells: HashMap<(i32, i32), Vec<CellUnit>>,
    hunts: BTreeMap<EntityId, HuntJob>,
    due: BTreeSet<(u64, EntityId)>,
    stats: WildlifeStats,
}
impl Wildlife {
    pub(crate) fn is_idle(&self) -> bool {
        self.animals.is_empty() && self.hunts.is_empty() && self.due.is_empty()
    }
}

pub(super) fn cell(at: WorldPosition) -> (i32, i32) {
    let tile = at.tile_floor();
    (tile.x.div_euclid(CELL_TILES), tile.y.div_euclid(CELL_TILES))
}
fn cell_of(tile: TileCoord) -> (i32, i32) {
    (tile.x.div_euclid(CELL_TILES), tile.y.div_euclid(CELL_TILES))
}
fn distance(a: WorldPosition, b: WorldPosition) -> i64 {
    let (x, y) = (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    );
    x * x + y * y
}
fn radius(tiles: i64) -> i64 {
    (tiles * i64::from(aoe_core::FIXED_SUBUNITS_PER_TILE)).pow(2)
}

impl GameWorld {
    pub(crate) fn track_unit_cell(&mut self, unit: &GameUnit) {
        self.wildlife
            .unit_cells
            .entry(cell(unit.position))
            .or_default()
            .push(CellUnit {
                id: unit.id,
                position: unit.position,
                player: unit.player,
                kind: unit.kind,
                alive: true,
            });
    }

    /// Movement calls this after each step; `previous_tile` is where the unit
    /// started the step. The mirrored position stays exact even without a
    /// cell change, because animals score distances against it.
    pub(crate) fn track_unit_step(
        &mut self,
        id: EntityId,
        previous_tile: TileCoord,
        unit: &GameUnit,
    ) {
        let (old, new) = (cell_of(previous_tile), cell(unit.position));
        if old == new {
            if let Some(units) = self.wildlife.unit_cells.get_mut(&old)
                && let Some(slot) = units.iter().position(|u| u.id == id)
            {
                units[slot].position = unit.position;
            }
            return;
        }
        self.untrack_unit_cell(id, old);
        self.track_unit_cell(unit);
    }

    pub(crate) fn untrack_unit_cell(&mut self, id: EntityId, key: (i32, i32)) {
        if let Some(units) = self.wildlife.unit_cells.get_mut(&key) {
            if let Some(slot) = units.iter().position(|u| u.id == id) {
                units.swap_remove(slot);
            }
            if units.is_empty() {
                self.wildlife.unit_cells.remove(&key);
            }
        }
    }

    pub(crate) fn untrack_unit_at(&mut self, id: EntityId, at: WorldPosition) {
        let key = cell(at);
        self.untrack_unit_cell(id, key);
    }

    pub(crate) fn track_unit_death(&mut self, id: EntityId, at: WorldPosition) {
        if let Some(units) = self.wildlife.unit_cells.get_mut(&cell(at))
            && let Some(slot) = units.iter().position(|u| u.id == id)
        {
            units[slot].alive = false;
        }
    }

    pub(crate) fn stop_sheep(
        &mut self,
        player: PlayerId,
        id: EntityId,
    ) -> Result<(), EconomyError> {
        let animal = self
            .wildlife
            .animals
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        if animal.state.kind != AnimalKind::Sheep || animal.state.hp == 0 {
            return Err(EconomyError::InvalidTarget);
        }
        if animal.state.owner != Some(player) {
            return Err(EconomyError::NotOwner);
        }
        animal.herd = None;
        animal.speed_carry = 0;
        animal.state.moving = false;
        self.wildlife.engaged.remove(&id);
        Ok(())
    }

    pub fn issue_herd(
        &mut self,
        player: PlayerId,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), EconomyError> {
        let a = self.animal(id).ok_or(EconomyError::UnknownEntity)?;
        if a.kind != AnimalKind::Sheep || a.hp == 0 {
            return Err(EconomyError::InvalidTarget);
        }
        if a.owner != Some(player) {
            return Err(EconomyError::NotOwner);
        }
        if !self.config.valid_ground_position(destination)
            || !self.tile_passable(destination.tile_floor())
        {
            return Err(EconomyError::InvalidTarget);
        }
        let destination = self.config.snap_ground_position(destination);
        let animal = self
            .wildlife
            .animals
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        animal.herd = Some(destination);
        self.wildlife.engaged.insert(id);
        Ok(())
    }
    pub fn hunting_target(&self, id: EntityId) -> Option<EntityId> {
        self.wildlife
            .hunts
            .get(&id)
            .map(|job| job.animal)
            .or_else(|| self.economy.gatherers.get(&id).and_then(|job| job.animal))
    }
    pub fn animals(&self) -> impl Iterator<Item = &GameAnimal> {
        self.wildlife.animals.values().map(|a| &a.state)
    }
    pub fn animal(&self, id: EntityId) -> Option<&GameAnimal> {
        self.wildlife.animals.get(&id).map(|a| &a.state)
    }
    pub fn wildlife_stats(&self) -> WildlifeStats {
        self.wildlife.stats
    }
    pub(crate) fn cancel_hunt(&mut self, id: EntityId) {
        self.wildlife.hunts.remove(&id);
    }
    fn huntable(&self, player: PlayerId, id: EntityId) -> Result<GameAnimal, EconomyError> {
        let a = *self.animal(id).ok_or(EconomyError::InvalidTarget)?;
        if a.owner.is_some_and(|owner| owner != player) {
            return Err(EconomyError::NotOwner);
        }
        if a.hp == 0 && a.food_remaining == 0 {
            return Err(EconomyError::InvalidResource);
        }
        Ok(a)
    }
}
