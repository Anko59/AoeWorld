//! Match startup rules. Ordinary training has no population ceiling.
use super::MAX_MATCH_SLOTS;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_protocol::MAX_SUBSCRIBED_UNITS;
use aoe_simulation::{GameWorld, GameWorldError, WorldLimits};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error("navigation preparation failed: {0}")]
    Navigation(String),
    #[error("a match needs between 1 and {MAX_MATCH_SLOTS} slots")]
    SlotCount,
    #[error("map has no land start for slot {0}; use a larger or less watery map")]
    NoStart(usize),
    #[error("start_units exceeds the bounded startup memory budget")]
    StartUnits,
    #[error(transparent)]
    World(#[from] GameWorldError),
}

/// Startup extras must leave room for the standard four-unit kit in the
/// bounded initial view. This constrains setup requests, never later training.
const MAX_START_UNITS: u64 = MAX_SUBSCRIBED_UNITS as u64 - 4;
/// Tiles around a village searched for places to put its start units.
const SEED_RADIUS: i32 = 60;
/// Tiles at the village's centre kept clear of start units.
const SEED_HOLE: i32 = 4;

/// Extra units each player starts with, around the village. A scenario hook
/// for tests and scale proofs: the units are ordinary villagers and militia
/// (one militia in `villagers / militia + 1` placed units when both are set),
/// placed on free ground without cost. They count toward population.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StartUnits {
    #[serde(default)]
    pub villagers: u32,
    #[serde(default)]
    pub militia: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MatchRules {
    pub start_units: StartUnits,
    /// Seeds which village each slot gets (and the human's seat in it). `None`
    /// keeps the canonical layout; a hosted match gets a fresh seed.
    pub seed: Option<u64>,
}

impl MatchRules {
    pub fn new(start_units: Option<StartUnits>) -> Result<Self, MatchError> {
        let start_units = start_units.unwrap_or_default();
        let total = u64::from(start_units.villagers) + u64::from(start_units.militia);
        if total > MAX_START_UNITS {
            return Err(MatchError::StartUnits);
        }
        Ok(Self {
            start_units,
            seed: None,
        })
    }

    /// The same rules with the village layout drawn from `seed`.
    pub fn with_seed(self, seed: u64) -> Self {
        Self {
            seed: Some(seed),
            ..self
        }
    }

    /// World storage budgets for `players` seats.
    pub(super) fn limits(&self, players: usize) -> WorldLimits {
        WorldLimits::for_match(players)
    }
}

/// Tile offsets by increasing distance, skipping the village centre.
fn offsets() -> Vec<(i32, i32)> {
    let mut all = Vec::new();
    for dy in -SEED_RADIUS..=SEED_RADIUS {
        for dx in -SEED_RADIUS..=SEED_RADIUS {
            let d2 = dx * dx + dy * dy;
            if d2 <= SEED_RADIUS * SEED_RADIUS && dx.abs().max(dy.abs()) > SEED_HOLE {
                all.push((d2, dy, dx));
            }
        }
    }
    all.sort_unstable();
    all.into_iter().map(|(_, dy, dx)| (dx, dy)).collect()
}

/// Spawns the start units of every seat around its scout `scouts[i]`.
/// Returns how many were placed; tiles that refuse a unit are skipped.
pub(super) fn seed_units(
    world: &mut GameWorld,
    scouts: &[EntityId],
    rules: &MatchRules,
    cancelled: &dyn Fn() -> bool,
) -> Result<u64, MatchError> {
    let StartUnits { villagers, militia } = rules.start_units;
    if villagers == 0 && militia == 0 {
        return Ok(0);
    }
    let ring = offsets();
    let every = villagers / militia.max(1) + 1;
    let mut placed = 0;
    for (index, scout) in scouts.iter().enumerate() {
        if cancelled() {
            return Err(aoe_simulation::GameWorldError::StartSearchLimit.into());
        }
        let player = aoe_core::PlayerId(index as u16);
        let Some(center) = world.unit(*scout).map(|unit| unit.position.tile_floor()) else {
            continue;
        };
        let (mut want_villagers, mut want_militia) = (villagers, militia);
        for (step, (dx, dy)) in ring.iter().enumerate() {
            if want_villagers == 0 && want_militia == 0 {
                break;
            }
            let Ok(position) =
                WorldPosition::from_tile_center(TileCoord::new(center.x + dx, center.y + dy))
            else {
                continue;
            };
            let militia_turn =
                want_militia > 0 && (want_villagers == 0 || (step as u32).is_multiple_of(every));
            let spawned = if militia_turn {
                world.spawn_militia(player, position)
            } else {
                world.spawn_villager(player, position)
            };
            if spawned.is_ok() {
                placed += 1;
                if militia_turn {
                    want_militia -= 1;
                } else {
                    want_villagers -= 1;
                }
            }
        }
    }
    Ok(placed)
}
