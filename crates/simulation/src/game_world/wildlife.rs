//! Integer wildlife state, local AI and villager hunting jobs.
use super::{EconomyError, GameWorld};
use aoe_core::{EntityId, PlayerId, WorldPosition};
use std::collections::{BTreeMap, BTreeSet};

mod ai;
mod carcass;
mod hash;
mod hunting;
mod spawning;
mod table;
pub use table::{AnimalKind, WILDLIFE_RULES, WildlifeRules};

pub const MAX_ANIMALS: usize = 4096;
pub const MAX_HUNTERS: usize = 80;
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
}

#[derive(Clone, Copy, Debug)]
struct StoredAnimal {
    state: GameAnimal,
    home: WorldPosition,
    cooldown: u32,
    speed_carry: u64,
    decay_carry: u64,
}

#[derive(Clone, Copy, Debug)]
struct HuntJob {
    animal: EntityId,
    repath_tick: u64,
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
    hunts: BTreeMap<EntityId, HuntJob>,
    due: BTreeSet<(u64, EntityId)>,
    stats: WildlifeStats,
}
impl Wildlife {
    pub(crate) fn is_idle(&self) -> bool {
        self.animals.is_empty() && self.hunts.is_empty() && self.due.is_empty()
    }
}

fn cell(at: WorldPosition) -> (i32, i32) {
    let tile = at.tile_floor();
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
