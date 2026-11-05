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
/// Outer radius of slot 0's probe army ring.
const PROBE_ARMY_RADIUS: i32 = 110;

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
    /// An army for slot 0 only (the real-match march probe).
    #[serde(default)]
    pub probe_army: ProbeArmy,
}

/// Soldiers of several kinds placed for slot 0 only, around its village, so
/// a scripted human can march real mixed armies (`scale-match --march-probe`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProbeArmy {
    #[serde(default)]
    pub militia: u32,
    #[serde(default)]
    pub archers: u32,
    #[serde(default)]
    pub knights: u32,
    #[serde(default)]
    pub rams: u32,
    #[serde(default)]
    pub mangonels: u32,
}

impl ProbeArmy {
    fn total(&self) -> u64 {
        [
            self.militia,
            self.archers,
            self.knights,
            self.rams,
            self.mangonels,
        ]
        .iter()
        .map(|n| u64::from(*n))
        .sum()
    }

    /// The kinds to place, interleaved so every ring holds a mix.
    fn kinds(&self) -> Vec<aoe_simulation::UnitKind> {
        use aoe_simulation::UnitKind as K;
        let mut left = [
            (K::Militia, self.militia),
            (K::Archer, self.archers),
            (K::Knight, self.knights),
            (K::BatteringRam, self.rams),
            (K::Mangonel, self.mangonels),
        ];
        let mut kinds = Vec::new();
        while left.iter().any(|(_, n)| *n > 0) {
            for (kind, n) in &mut left {
                if *n > 0 {
                    kinds.push(*kind);
                    *n -= 1;
                }
            }
        }
        kinds
    }
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
        let total = u64::from(start_units.villagers)
            + u64::from(start_units.militia)
            + start_units.probe_army.total();
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
fn offsets(radius: i32) -> Vec<(i32, i32)> {
    let mut all = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = dx * dx + dy * dy;
            if d2 <= radius * radius && dx.abs().max(dy.abs()) > SEED_HOLE {
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
    let StartUnits {
        villagers,
        militia,
        probe_army,
    } = rules.start_units;
    let mut placed = seed_probe_army(world, scouts.first().copied(), probe_army);
    if villagers == 0 && militia == 0 {
        return Ok(placed);
    }
    let ring = offsets(SEED_RADIUS);
    let every = villagers / militia.max(1) + 1;
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

/// Places slot 0's probe army on free ground, every second tile of a ring
/// outside the ordinary start units, so the soldiers stand apart.
fn seed_probe_army(world: &mut GameWorld, scout: Option<EntityId>, army: ProbeArmy) -> u64 {
    let Some(center) = scout
        .and_then(|id| world.unit(id))
        .map(|u| u.position.tile_floor())
    else {
        return 0;
    };
    let mut kinds = army.kinds().into_iter().peekable();
    let mut placed = 0;
    for (dx, dy) in offsets(PROBE_ARMY_RADIUS) {
        let Some(&kind) = kinds.peek() else {
            break;
        };
        if (dx + dy) % 2 != 0 || dx * dx + dy * dy <= SEED_RADIUS * SEED_RADIUS {
            continue;
        }
        let tile = TileCoord::new(center.x + dx, center.y + dy);
        let Ok(position) = WorldPosition::from_tile_center(tile) else {
            continue;
        };
        if world
            .spawn_kind(aoe_core::PlayerId(0), kind, position)
            .is_ok()
        {
            kinds.next();
            placed += 1;
        }
    }
    placed
}
