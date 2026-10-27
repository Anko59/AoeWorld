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
    orders_per_domain: 2,
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
    pub site_checks: usize,
}
#[derive(Default)]
pub(super) struct Budget {
    pub orders: usize,
    pub searches: usize,
    pub sites: usize,
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
        work.resource_searches += 1;
        true
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
