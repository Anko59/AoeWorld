//! Deterministic work limits, independent of machine speed and wall time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VassalRules {
    pub radius_tiles: i32,
    pub max_villagers: usize,
    pub period_ticks: u64,
    pub domains_per_pass: usize,
    pub unit_visits_per_domain: usize,
    pub building_visits_per_domain: usize,
    pub orders_per_domain: usize,
    pub resource_searches_per_domain: usize,
    pub site_checks_per_domain: usize,
}
pub const VASSAL_RULES: VassalRules = VassalRules {
    radius_tiles: 100,
    max_villagers: 100,
    period_ticks: 20,
    domains_per_pass: 8,
    unit_visits_per_domain: 1_024,
    building_visits_per_domain: 2_048,
    orders_per_domain: 64,
    resource_searches_per_domain: 8,
    site_checks_per_domain: 32,
};
/// Counts for the most recent tick, including refused command attempts.
/// Ordinary movement/pathfinding retains its own shared simulation budgets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VassalWork {
    pub domains: usize,
    pub unit_visits: usize,
    pub building_visits: usize,
    pub orders: usize,
    pub resource_searches: usize,
    /// Candidate block lists inspected by the Vassal's local queries/discovery.
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
    /// After one productive change, only fourfold shortages may move more.
    pub reassigned: bool,
    jobs: usize,
    node_credits: usize,
}
impl Budget {
    pub fn order(&mut self, work: &mut VassalWork) -> bool {
        if self.orders >= VASSAL_RULES.orders_per_domain {
            return false;
        }
        self.orders += 1;
        work.orders += 1;
        true
    }
    pub fn search(&mut self, work: &mut VassalWork) -> bool {
        if self.searches >= VASSAL_RULES.resource_searches_per_domain {
            return false;
        }
        self.searches += 1;
        // A fixed finder visits at most25 lists of64nodes, plus8 cache checks.
        self.node_credits += 1_608;
        work.resource_searches += 1;
        true
    }
    /// Failed productive probes are bounded too, even with no admitted order.
    pub fn support_available(&self) -> bool {
        self.orders < 2
    }
    pub fn job(&mut self) -> bool {
        if self.jobs >= VASSAL_RULES.orders_per_domain {
            return false;
        }
        self.jobs += 1;
        true
    }
    /// Reuse unused finder allowance without granting another map query.
    pub fn inspect(&mut self, work: &mut VassalWork, count: usize) -> bool {
        if count > self.node_credits {
            return false;
        }
        self.node_credits -= count;
        work.resource_nodes += count;
        true
    }
    /// Each fresh bounded query funds its own finder and cache inspection.
    pub fn queried_nodes(&mut self, work: &mut VassalWork, count: usize) {
        assert!(
            self.inspect(work, count),
            "bounded finder exceeded its allowance"
        );
    }
    pub fn site(&mut self, work: &mut VassalWork) -> bool {
        if self.sites >= VASSAL_RULES.site_checks_per_domain {
            return false;
        }
        self.sites += 1;
        work.site_checks += 1;
        true
    }
}
/// Kingdom default weights for food, wood, gold and stone (45:30:15:10).
pub const DEFAULT_MIX: [u8; 4] = [45, 30, 15, 10];

/// Owner-set controls of one Vassal. A weight scales how strongly the kingdom's
/// reserve comparison favours that resource (lowest reserve per weight first);
/// a weight of zero makes it a last resort. The cap bounds managed villagers,
/// and training stays off until the owner turns it on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VassalSettings {
    pub weights: [u8; 4],
    pub cap: u8,
    pub training: bool,
}

impl Default for VassalSettings {
    fn default() -> Self {
        Self {
            weights: DEFAULT_MIX,
            cap: VASSAL_RULES.max_villagers as u8,
            training: false,
        }
    }
}

impl VassalSettings {
    /// Every weight and the cap lie within 0..=100, and some weight is positive.
    pub fn is_valid(&self) -> bool {
        self.cap <= 100
            && self.weights.iter().all(|w| *w <= 100)
            && self.weights.iter().any(|w| *w > 0)
    }
}
