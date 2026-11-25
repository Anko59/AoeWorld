//! Deterministic Steward rules, independent of machine speed and wall time.
//! The per-visit counts of orders (64), resource searches (8) and site checks
//! (32) are gone: orders, searches and site checks follow the visit's idle
//! villagers and missing camps. Bounds that remain: support actions (training,
//! houses, reseeds, farm repair) run only while fewer than 2 orders were
//! issued this visit (`support_available`); each search funds a fixed node
//! allowance (`node_credits`, 1,608 nodes) for the finder and cached offers;
//! a census or building sweep visits at most 1,024 units and 2,048 buildings
//! per visit, and runs only after activation or a zone change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StewardRules {
    pub radius_tiles: i32,
    pub max_villagers: usize,
    /// Reference ticks between two visits of one domain (`WorldConfig::ticks`):
    /// one second, whatever the number of domains in the world.
    pub period_ticks: u64,
    /// Minimum productive job tenure before a shortage rebalance.
    pub worker_dwell_seconds: u64,
    pub unit_visits_per_domain: usize,
    pub building_visits_per_domain: usize,
}
pub const STEWARD_RULES: StewardRules = StewardRules {
    radius_tiles: 100,
    max_villagers: 100,
    period_ticks: 20,
    worker_dwell_seconds: 20,
    unit_visits_per_domain: 1_024,
    building_visits_per_domain: 2_048,
};
/// Counts for the most recent tick, including refused command attempts.
/// Ordinary movement/pathfinding retains its own shared simulation budgets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StewardWork {
    pub domains: usize,
    pub unit_visits: usize,
    pub building_visits: usize,
    pub orders: usize,
    pub resource_searches: usize,
    /// Candidate block lists inspected by the Steward's local queries/discovery.
    pub resource_blocks: usize,
    /// Every finder list/cache entry inspected, including rejected/depleted nodes.
    /// Cold list generation (starting history and 64 terrain probes) and
    /// ordinary command admission retain their existing separate work bounds.
    pub resource_nodes: usize,
    pub site_checks: usize,
}
#[derive(Default)]
pub(super) struct Budget {
    pub orders: usize,
    pub searches: usize,
    pub sites: usize,
    node_credits: usize,
}
impl Budget {
    /// Counts an order. A visit issues one per event it handles (an idle
    /// villager, a missing camp or farm), never up to a fixed count.
    pub fn order(&mut self, work: &mut StewardWork) -> bool {
        self.orders += 1;
        work.orders += 1;
        true
    }
    pub fn search(&mut self, work: &mut StewardWork) {
        self.searches += 1;
        // A fixed finder visits at most 25 lists of 64 nodes, plus 8 cache checks.
        self.node_credits += 1_608;
        work.resource_searches += 1;
    }
    /// Failed productive probes are bounded too, even with no admitted order.
    pub fn support_available(&self) -> bool {
        self.orders < 2
    }
    /// Reuse unused finder allowance without granting another map query.
    pub fn inspect(&mut self, work: &mut StewardWork, count: usize) -> bool {
        if count > self.node_credits {
            return false;
        }
        self.node_credits -= count;
        work.resource_nodes += count;
        true
    }
    /// Each fresh bounded query funds its own finder and cache inspection.
    pub fn queried_nodes(&mut self, work: &mut StewardWork, count: usize) {
        assert!(
            self.inspect(work, count),
            "bounded finder exceeded its allowance"
        );
    }
    pub fn site(&mut self, work: &mut StewardWork) {
        self.sites += 1;
        work.site_checks += 1;
    }
}
/// Kingdom default weights for food, wood, gold and stone (45:30:15:10).
pub const DEFAULT_MIX: [u8; 4] = [45, 30, 15, 10];

/// Owner-set controls of one Steward. A weight scales how strongly the kingdom's
/// reserve comparison favours that resource (lowest reserve per weight first);
/// a weight of zero makes it a last resort. The cap bounds managed villagers,
/// and training stays off until the owner turns it on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StewardSettings {
    pub weights: [u8; 4],
    pub cap: u8,
    pub training: bool,
}

impl Default for StewardSettings {
    fn default() -> Self {
        Self {
            weights: DEFAULT_MIX,
            cap: STEWARD_RULES.max_villagers as u8,
            training: false,
        }
    }
}

impl StewardSettings {
    /// Every weight and the cap lie within 0..=100, and some weight is positive.
    pub fn is_valid(&self) -> bool {
        self.cap <= 100
            && self.weights.iter().all(|w| *w <= 100)
            && self.weights.iter().any(|w| *w > 0)
    }
}

/// Replay-only inclusive spans around the parts of a Steward pass.
pub const STEWARD_SPANS: [&str; 29] = [
    "steward_walk",
    "steward_members",
    "steward_transfers",
    "steward_regions",
    "steward_release",
    "steward_snapshot",
    "steward_send_away",
    "steward_adopt",
    "steward_watchdog",
    "steward_discover",
    "steward_navy",
    "steward_support",
    "steward_rebalance",
    "steward_idle",
    "steward_job",
    "steward_census",
    "steward_j_reclaim_farm",
    "steward_j_food",
    "steward_j_help_foundation",
    "steward_j_build_local",
    "steward_j_resource",
    "steward_j_camp_resource",
    "steward_members_copy",
    "steward_building_sweep",
    "steward_building_copy",
    "steward_farms",
    "steward_wood",
    "steward_retire",
    "steward_relocate",
];

impl crate::GameWorld {
    pub(super) fn steward_span(
        &self,
        name: &'static str,
    ) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        self.planning_probe
            .as_ref()
            .and_then(|probe| probe.movement_span(name))
    }
}
