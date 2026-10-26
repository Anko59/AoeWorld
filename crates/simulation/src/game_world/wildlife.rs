//! Integer wildlife state, local AI and villager hunting jobs.
use super::{EconomyError, GameUnit, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod ai;
mod carcass;
mod continuation;
mod hash;
mod hunting;
mod lure;
mod spawning;
mod table;
pub use table::{AnimalKind, WILDLIFE_RULES, WildlifeRules};

/// Animals per seat: the 15 of a village's wildlife kit, with room to spare.
pub const ANIMALS_PER_PLAYER: usize = 32;
/// The animal capacity of a world with every seat of the default limits.
pub const MAX_ANIMALS: usize = super::economy::MAX_ECONOMY_PLAYERS * ANIMALS_PER_PLAYER;
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
    stage: HuntStage,
}

/// AoE2 shepherding: walk to the sheep, lead it to the nearest food drop-off,
/// then kill it there. Other prey (and a sheep that cannot be led) is killed
/// where it stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HuntStage {
    Approach,
    Lead(WorldPosition),
    Kill,
}

/// A live unit mirrored into the wildlife cell grid, so animals can find
/// nearby units through point lookups instead of scanning the population.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CellUnit {
    pub(super) id: EntityId,
    pub(super) position: WorldPosition,
    player: PlayerId,
    pub(super) kind: UnitKind,
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
    /// Actual local cell and animal visits for depleted-food replacement.
    pub food_search_cells: u32,
    pub food_search_candidates: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Wildlife {
    animals: BTreeMap<EntityId, StoredAnimal>,
    /// Living animals and retained carcasses, for local food replacement.
    cells: BTreeMap<(i32, i32), BTreeSet<EntityId>>,
    /// Chasing/fleeing animals get a cleanup step if nearby units disappear.
    engaged: BTreeSet<EntityId>,
    /// Units mirrored by wildlife cell. Only point lookups and per-cell
    /// edits are made, so the hash map never biases a scan order.
    unit_cells: HashMap<u64, Vec<CellUnit>>,
    hunts: BTreeMap<EntityId, HuntJob>,
    due: BTreeSet<(u64, EntityId)>,
    stats: WildlifeStats,
}
impl Wildlife {
    /// Ids that currently hold a hunt job.
    pub(crate) fn hunter_ids(&self) -> impl Iterator<Item = &EntityId> {
        self.hunts.keys()
    }

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
// One integer hash instead of two field hashes. These maps use point lookups
// only; packing coordinates never changes the order inside a cell's vector.
fn unit_key((x, y): (i32, i32)) -> u64 {
    u64::from(x as u32) | (u64::from(y as u32) << 32)
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
    /// Four local eight-tile cells at most, with 32 inspections per cell.
    /// Rotate a deterministic window in crowded cells so dense stacks do not
    /// turn pair work quadratic or permanently hide the same neighbours.
    pub(crate) fn spacing_neighbours(
        &self,
        at: WorldPosition,
        salt: u64,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.spacing_units(at, salt, EntityId(u32::MAX))
            .map(|unit| unit.id)
    }

    /// Reuse the live cell mirror directly; isolated actors create no windows
    /// and candidates need no second random lookup into the population store.
    pub(super) fn spacing_units(
        &self,
        at: WorldPosition,
        salt: u64,
        except: EntityId,
    ) -> impl Iterator<Item = &CellUnit> + '_ {
        let tile = at.tile_floor();
        let min = cell_of(TileCoord::new(tile.x - 1, tile.y - 1));
        let max = cell_of(TileCoord::new(tile.x + 1, tile.y + 1));
        let min_x = at.x.saturating_sub(1023);
        let max_x = at.x.saturating_add(1023);
        let min_y = at.y.saturating_sub(1023);
        let max_y = at.y.saturating_add(1023);
        // A rotated window is at most two contiguous slices. Walk those
        // directly: no modulo or nested iterator stack per inspected unit.
        let mut windows: [&[CellUnit]; 8] = [&[]; 8];
        let mut count = 0;
        for y in min.1..=max.1 {
            for x in min.0..=max.0 {
                let units = self
                    .wildlife
                    .unit_cells
                    .get(&unit_key((x, y)))
                    .map_or(&[][..], Vec::as_slice);
                if units.is_empty() || (units.len() == 1 && units[0].id == except) {
                    continue;
                }
                let start = (salt % units.len() as u64) as usize;
                let limit = units.len().min(32);
                let first = limit.min(units.len() - start);
                if first > 0 {
                    windows[count] = &units[start..start + first];
                    count += 1;
                }
                if limit > first {
                    windows[count] = &units[..limit - first];
                    count += 1;
                }
            }
        }
        let mut window = 0;
        let mut offset = 0;
        std::iter::from_fn(move || {
            while window < count {
                let units = windows[window];
                if offset >= units.len() {
                    window += 1;
                    offset = 0;
                    continue;
                }
                let unit = &units[offset];
                offset += 1;
                if unit.id != except
                    && unit.alive
                    && unit.position.x >= min_x
                    && unit.position.x <= max_x
                    && unit.position.y >= min_y
                    && unit.position.y <= max_y
                {
                    return Some(unit);
                }
            }
            None
        })
    }

    pub(crate) fn track_unit_cell(&mut self, unit: &GameUnit) {
        let units = self
            .wildlife
            .unit_cells
            .entry(unit_key(cell(unit.position)))
            .or_default();
        self.units[self.lookup[&unit.id]].wildlife_slot = units.len();
        units.push(CellUnit {
            id: unit.id,
            position: unit.position,
            player: unit.player,
            kind: unit.kind,
            alive: true,
        });
    }

    /// The dense unit index supplies its cell slot: even a completely stacked
    /// crowd updates its spatial mirror in O(1), without searching the bucket.
    pub(crate) fn track_unit_step(
        &mut self,
        id: EntityId,
        previous_tile: TileCoord,
        unit: &GameUnit,
    ) {
        if self.followers.contains(id) {
            self.followers
                .relocate(id, previous_tile, unit.position.tile_floor());
        } else if self.units[self.lookup[&id]].formation.is_none() {
            self.idle_spacing
                .relocate(id, previous_tile, unit.position.tile_floor());
        }
        let (old, new) = (cell_of(previous_tile), cell(unit.position));
        if old == new {
            let slot = self.units[self.lookup[&id]].wildlife_slot;
            if let Some(units) = self.wildlife.unit_cells.get_mut(&unit_key(old))
                && let Some(entry) = units.get_mut(slot).filter(|u| u.id == id)
            {
                entry.position = unit.position;
            }
            return;
        }
        self.untrack_unit_cell(id, old);
        self.track_unit_cell(unit);
    }

    pub(crate) fn untrack_unit_cell(&mut self, id: EntityId, key: (i32, i32)) {
        let key = unit_key(key);
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let slot = self.units[index].wildlife_slot;
        let Some(units) = self.wildlife.unit_cells.get_mut(&key) else {
            return;
        };
        if units.get(slot).is_none_or(|u| u.id != id) {
            return;
        }
        units.swap_remove(slot);
        self.units[index].wildlife_slot = usize::MAX;
        if let Some(displaced) = units.get(slot) {
            self.units[self.lookup[&displaced.id]].wildlife_slot = slot;
        }
        if units.is_empty() {
            self.wildlife.unit_cells.remove(&key);
        }
    }

    pub(crate) fn untrack_unit_at(&mut self, id: EntityId, at: WorldPosition) {
        self.idle_spacing.remove(id, at.tile_floor());
        self.followers.remove(id, at.tile_floor());
        self.untrack_unit_cell(id, cell(at));
    }

    pub(crate) fn track_unit_death(&mut self, id: EntityId, at: WorldPosition) {
        self.idle_spacing.remove(id, at.tile_floor());
        self.followers.remove(id, at.tile_floor());
        let slot = self.units[self.lookup[&id]].wildlife_slot;
        if let Some(units) = self.wildlife.unit_cells.get_mut(&unit_key(cell(at)))
            && let Some(entry) = units.get_mut(slot).filter(|u| u.id == id)
        {
            entry.alive = false;
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
    /// Every hunter in id order with the animal it hunts.
    pub fn hunts(&self) -> impl Iterator<Item = (EntityId, EntityId)> + '_ {
        self.wildlife
            .hunts
            .iter()
            .map(|(id, job)| (*id, job.animal))
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
    pub(crate) fn huntable(
        &self,
        player: PlayerId,
        id: EntityId,
    ) -> Result<GameAnimal, EconomyError> {
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
