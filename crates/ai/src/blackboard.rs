//! What a bot remembers between decisions (`ai/aoe_ai/blackboard.py`).
//! Maps and sets are ordered, so every iteration is deterministic.
use std::collections::{BTreeMap, BTreeSet};

use aoe_sdk::{BuildingKind, Order, ResourceKind};

use crate::view::Tile;

/// An order whose verdict has not arrived yet.
#[derive(Clone, Debug, PartialEq)]
pub struct Tracked {
    pub name: String,
    pub order: Order,
    pub tick: u64,
}

/// A building the bot intends to have (placed but maybe not yet visible).
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub kind: BuildingKind,
    pub tile: Tile,
    pub tick: u64,
}

/// Index of a resource kind in the per-resource tables.
pub fn slot(kind: ResourceKind) -> usize {
    match kind {
        ResourceKind::Food => 0,
        ResourceKind::Wood => 1,
        ResourceKind::Gold => 2,
        ResourceKind::Stone => 3,
    }
}

/// The Town Centers and home last seen, and each tile's distance to them.
#[derive(Clone, Debug, Default)]
pub struct Settlements {
    pub sites: Option<(Vec<Tile>, Option<Tile>)>,
    pub distances: BTreeMap<Tile, f64>,
}

/// The dropoff sites last seen for a resource and each node tile's distance
/// to the nearest of them.
pub type Dropoffs = (Vec<Tile>, BTreeMap<Tile, f64>);

/// Everything the managers share between decisions: orders in flight,
/// refusals learned, plans, scouting progress and the attack state.
#[derive(Clone, Debug)]
pub struct Blackboard {
    pub tracked: BTreeMap<u64, Tracked>,
    /// Resource nodes (or animals) a gather or hunt order was refused for.
    pub bad_resources: BTreeSet<u64>,
    /// (kind name, x, y) sites the server refused.
    pub bad_sites: BTreeSet<(String, i32, i32)>,
    /// Unit kinds the server refused to train (cleared when the age changes).
    pub blocked_units: BTreeSet<String>,
    pub plans: Vec<Plan>,
    /// farm id -> farmer id
    pub farmers: BTreeMap<u32, u32>,
    /// villager id -> (what, tile) job description while a build is under way
    pub builders: BTreeMap<u32, (String, Tile)>,
    pub wood_dropoff_cursor: usize,
    pub scout_waypoints: Vec<Tile>,
    pub scout_index: usize,
    pub scout_radius: i32,
    pub scout_stage: u32,
    pub march_goal: Option<Tile>,
    pub march_anchor: Option<Tile>,
    pub march_since: u64,
    /// soldier id -> tick of its last march order
    pub marched: BTreeMap<u32, u64>,
    /// soldier id -> (last progress tile, tick)
    pub march_progress: BTreeMap<u32, (Tile, u64)>,
    /// villager id -> (resource node, tick) of its last gather order
    pub gather_sent: BTreeMap<u32, (u64, u64)>,
    /// scout id -> the waypoint it was last sent to
    pub scout_sent: BTreeMap<u32, usize>,
    pub scout_targets: BTreeMap<u32, Tile>,
    pub scout_progress: BTreeMap<u32, (Tile, u64)>,
    pub home: Option<Tile>,
    /// Ticks until which a thing is not tried again, to avoid hammering refusals.
    pub cooldowns: BTreeMap<String, u64>,
    pub threat_until: i64,
    pub threat_tile: Option<Tile>,
    pub raid_tile: Option<Tile>,
    pub raid_started: i64,
    pub raid_until: i64,
    pub raid_reach: BTreeMap<u32, bool>,
    pub counter_owner: Option<u16>,
    pub counter_bearing: Option<Tile>,
    pub counter_until: i64,
    pub march_after: BTreeMap<String, (i64, i64, i64)>,
    pub army_view_until: i64,
    pub attack_launch_size: u32,
    pub attack_target: Option<u32>,
    pub attacking: bool,
    pub attack_members: BTreeSet<u32>,
    pub search_index: usize,
    /// Destinations a move order was refused for.
    pub bad_moves: BTreeSet<Tile>,
    /// Failed land queries, with the tick from which they are retried.
    pub land_failures: BTreeMap<(Tile, Tile), u64>,
    /// villager id -> tick of its last job
    pub assigned: BTreeMap<u32, u64>,
    pub idle_after: i64,
    pub worker_view_until: i64,
    /// Worker -> (Steward, first idle tick)
    pub steward_idle: BTreeMap<u32, (u32, u64)>,
    /// (villager, animal) -> hunt orders given
    pub hunt_tries: BTreeMap<(u32, u32), u32>,
    /// Per resource (see `slot`): the dropoff sites and distances last seen.
    pub dropoffs: [Option<Dropoffs>; 4],
    pub settlements: Settlements,
    /// villager id -> animal it was sent after
    pub hunters: BTreeMap<u32, u32>,
    /// animal id -> ((hp, meat), tick it last changed)
    pub hunt_watch: BTreeMap<u32, ((u16, u16), u64)>,
    /// Enemy units and buildings seen below full health (our hits landing).
    pub wounded: BTreeSet<u32>,
    /// Own foundation id -> (progress, tick it last advanced)
    pub progress: BTreeMap<u32, (u16, u64)>,
    /// Enemy unit or building id -> hit points when last seen.
    pub enemy_hp: BTreeMap<u32, u16>,
    /// Counters for reports and tests.
    pub stats: BTreeMap<String, u64>,
    /// Management unit -> building it was sent to garrison in.
    pub seated: BTreeMap<u32, u32>,
    pub generals: BTreeSet<u32>,
    /// Self-limiting saving: key -> tick it began.
    pub save_since: BTreeMap<String, u64>,
    pub saving_tick: i64,
    /// Management units ordered and still training: kind -> [(tick, units existing then)].
    pub producing: BTreeMap<String, Vec<(u64, usize)>>,
    pub vassal_workers: BTreeMap<u32, BTreeSet<u32>>,
    /// Steward or Vassal id -> (weights, cap, training) last sent.
    pub manager_settings: BTreeMap<u32, ([u8; 4], u8, bool)>,
    pub general_training: BTreeMap<u32, bool>,
    pub military_cursor: usize,
    pub military_rally_cursor: usize,
    pub production_cursor: usize,
    pub expansion_cursor: usize,
    pub hold_villagers_tick: i64,
    pub fort_cursor: usize,
    pub defence_alerts: BTreeMap<Tile, u64>,
    pub defence_buildings: BTreeMap<u32, Tile>,
    pub fort_lanes: BTreeMap<Tile, Tile>,
    /// Sampled approach points whose gate footprint was refused, per settlement.
    pub fort_rejected: BTreeMap<Tile, BTreeSet<Tile>>,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self {
            tracked: BTreeMap::new(),
            bad_resources: BTreeSet::new(),
            bad_sites: BTreeSet::new(),
            blocked_units: BTreeSet::new(),
            plans: Vec::new(),
            farmers: BTreeMap::new(),
            builders: BTreeMap::new(),
            wood_dropoff_cursor: 0,
            scout_waypoints: Vec::new(),
            scout_index: 0,
            scout_radius: 0,
            scout_stage: 0,
            march_goal: None,
            march_anchor: None,
            march_since: 0,
            marched: BTreeMap::new(),
            march_progress: BTreeMap::new(),
            gather_sent: BTreeMap::new(),
            scout_sent: BTreeMap::new(),
            scout_targets: BTreeMap::new(),
            scout_progress: BTreeMap::new(),
            home: None,
            cooldowns: BTreeMap::new(),
            threat_until: -1,
            threat_tile: None,
            raid_tile: None,
            raid_started: -1,
            raid_until: -1,
            raid_reach: BTreeMap::new(),
            counter_owner: None,
            counter_bearing: None,
            counter_until: -1,
            march_after: BTreeMap::new(),
            army_view_until: 0,
            attack_launch_size: 0,
            attack_target: None,
            attacking: false,
            attack_members: BTreeSet::new(),
            search_index: 0,
            bad_moves: BTreeSet::new(),
            land_failures: BTreeMap::new(),
            assigned: BTreeMap::new(),
            idle_after: -1,
            worker_view_until: 0,
            steward_idle: BTreeMap::new(),
            hunt_tries: BTreeMap::new(),
            dropoffs: Default::default(),
            settlements: Settlements::default(),
            hunters: BTreeMap::new(),
            hunt_watch: BTreeMap::new(),
            wounded: BTreeSet::new(),
            progress: BTreeMap::new(),
            enemy_hp: BTreeMap::new(),
            stats: BTreeMap::new(),
            seated: BTreeMap::new(),
            generals: BTreeSet::new(),
            save_since: BTreeMap::new(),
            saving_tick: -1_000_000_000,
            producing: BTreeMap::new(),
            vassal_workers: BTreeMap::new(),
            manager_settings: BTreeMap::new(),
            general_training: BTreeMap::new(),
            military_cursor: 0,
            military_rally_cursor: 0,
            production_cursor: 0,
            expansion_cursor: 0,
            hold_villagers_tick: -1_000_000_000,
            fort_cursor: 0,
            defence_alerts: BTreeMap::new(),
            defence_buildings: BTreeMap::new(),
            fort_lanes: BTreeMap::new(),
            fort_rejected: BTreeMap::new(),
        }
    }
}

impl Blackboard {
    pub fn bump(&mut self, name: &str) {
        *self.stats.entry(name.to_owned()).or_insert(0) += 1;
    }

    /// Whether `name` is cooling at `tick` (it holds until its last tick).
    pub fn cooling(&self, name: &str, tick: u64) -> bool {
        self.cooldowns.get(name).is_some_and(|until| *until > tick)
    }

    pub fn cool(&mut self, name: &str, tick: u64, ticks: u64) {
        // Expired cooldowns are dropped, so the table is bounded by what is
        // cooling now, not by the length of the game.
        if self.cooldowns.len() >= 2048 {
            self.cooldowns.retain(|_, until| *until > tick);
        }
        self.cooldowns.insert(name.to_owned(), tick + ticks);
    }
}
