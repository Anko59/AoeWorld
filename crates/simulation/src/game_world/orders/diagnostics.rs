//! Movement demand, independent of deterministic game state and save data.
use super::*;

#[cfg(test)]
#[path = "tests/diagnostics.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(usize)]
pub enum OrderIssuer {
    Player,
    PythonAi,
    AutoScout,
    Steward,
    Vassal,
    General,
    #[default]
    Automation,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct OrderDemand {
    /// Source decisions (external commands, manager orders, automatic trip starts).
    pub decisions: u64,
    /// Unit movement intents, including refused and redundant requests.
    pub orders: u64,
    pub near_reissues: u64,
    /// Accepted replacements within ten simulation seconds of the last order.
    pub reassigned_10s: u64,
    /// Accepted orders replacing an unfinished movement, charged to its issuer.
    pub superseded: u64,
    /// Unfinished movement replaced before ten simulation seconds elapsed.
    pub superseded_10s: u64,
    /// Calls to route admission (including cache hits and slot refusals).
    pub route_requests: u64,
    /// Fresh route planners at issue, excluding navigation-cache hits.
    pub route_searches: u64,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct OrderRedundancy {
    pub near_reissues: u64,
    pub superseded_10s: u64,
}

type Site = (&'static str, u32);
type Source = (usize, Option<Site>, Site, usize);

#[derive(Debug, Default)]
pub(crate) struct OrderDiagnostics {
    pub issuer: OrderIssuer,
    pub demand: [OrderDemand; 7],
    last: BTreeMap<EntityId, (u64, OrderIssuer, Site)>,
    sources: BTreeMap<Source, OrderRedundancy>,
}

impl GameWorld {
    /// Attribute nested synchronous decisions; the outer issuer is restored.
    pub fn with_order_issuer<T>(
        &mut self,
        issuer: OrderIssuer,
        decide: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = std::mem::replace(&mut self.order_diagnostics.issuer, issuer);
        let result = decide(self);
        self.order_diagnostics.issuer = previous;
        result
    }

    pub fn record_order_decisions(&mut self, count: u64) {
        self.order_diagnostics.demand[self.order_diagnostics.issuer as usize].decisions += count;
    }

    pub(crate) fn record_vassal_decision(&mut self) {
        if self.order_diagnostics.issuer == OrderIssuer::Vassal {
            self.record_order_decisions(1);
        }
    }

    pub fn order_demand(&self) -> &[OrderDemand; 7] {
        &self.order_diagnostics.demand
    }

    /// Redundant requests by charged issuer, old request site and new caller.
    /// These observations never participate in movement decisions or saves.
    pub fn order_redundancy_sources(&self) -> BTreeMap<String, OrderRedundancy> {
        let issuers = [
            "player",
            "python_ai",
            "auto_scout",
            "steward",
            "vassal",
            "general",
            "rally_automation",
        ];
        self.order_diagnostics
            .sources
            .iter()
            .map(|(&(charged, old, new, current), &counts)| {
                let old = old.unwrap_or(("restored", 0));
                (
                    format!(
                        "{} -> {} | {}:{} -> {}:{}",
                        issuers[charged], issuers[current], old.0, old.1, new.0, new.1
                    ),
                    counts,
                )
            })
            .collect()
    }

    pub(crate) fn forget_order_demand(&mut self, id: EntityId) {
        self.order_diagnostics.last.remove(&id);
    }

    #[track_caller]
    pub(crate) fn demand_attempt(&mut self, id: EntityId, destination: WorldPosition) {
        if self.order_diagnostics.issuer == OrderIssuer::Automation {
            self.record_order_decisions(1);
        }
        let near = self
            .movement_goal(id)
            .is_some_and(|goal| near_goal(goal, destination));
        let row = &mut self.order_diagnostics.demand[self.order_diagnostics.issuer as usize];
        row.orders += 1;
        row.near_reissues += u64::from(near);
        if near {
            let caller = std::panic::Location::caller();
            let issuer = self.order_diagnostics.issuer as usize;
            let previous = self.order_diagnostics.last.get(&id).map(|p| p.2);
            self.order_diagnostics
                .sources
                .entry((issuer, previous, (caller.file(), caller.line()), issuer))
                .or_default()
                .near_reissues += 1;
        }
    }

    #[track_caller]
    pub(crate) fn demand_cancelled(&mut self, id: EntityId) {
        if self.movement_goal(id).is_some()
            && let Some((tick, issuer, previous)) = self.order_diagnostics.last.get(&id).copied()
        {
            let row = &mut self.order_diagnostics.demand[issuer as usize];
            row.superseded += 1;
            row.superseded_10s +=
                u64::from(self.tick.0.saturating_sub(tick) < 10 * u64::from(self.config.tick_hz));
            if self.tick.0.saturating_sub(tick) < 10 * u64::from(self.config.tick_hz) {
                self.record_superseded_source(issuer, previous);
            }
        }
    }

    #[track_caller]
    pub(crate) fn demand_accepted(&mut self, id: EntityId) {
        let unfinished = self.movement_goal(id).is_some();
        let issuer = self.order_diagnostics.issuer;
        let caller = std::panic::Location::caller();
        if let Some((tick, previous, site)) = self
            .order_diagnostics
            .last
            .insert(id, (self.tick.0, issuer, (caller.file(), caller.line())))
        {
            if self.tick.0.saturating_sub(tick) < 10 * u64::from(self.config.tick_hz) {
                self.order_diagnostics.demand[issuer as usize].reassigned_10s += 1;
            }
            if unfinished {
                let row = &mut self.order_diagnostics.demand[previous as usize];
                row.superseded += 1;
                row.superseded_10s += u64::from(
                    self.tick.0.saturating_sub(tick) < 10 * u64::from(self.config.tick_hz),
                );
                if self.tick.0.saturating_sub(tick) < 10 * u64::from(self.config.tick_hz) {
                    self.record_superseded_source(previous, site);
                }
            }
        }
    }

    #[track_caller]
    fn record_superseded_source(&mut self, previous: OrderIssuer, site: Site) {
        let caller = std::panic::Location::caller();
        let issuer = self.order_diagnostics.issuer as usize;
        self.order_diagnostics
            .sources
            .entry((
                previous as usize,
                Some(site),
                (caller.file(), caller.line()),
                issuer,
            ))
            .or_default()
            .superseded_10s += 1;
    }

    pub(crate) fn movement_goal(&self, id: EntityId) -> Option<WorldPosition> {
        let unit = &self.units[*self.lookup.get(&id)?];
        if let Some(leader) = unit.formation {
            return self.formation_movement_goal(id, leader);
        }
        unit.order.map(|order| order.destination)
    }
}

pub(crate) fn near_goal(a: WorldPosition, b: WorldPosition) -> bool {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy <= i64::from(FIXED_SUBUNITS_PER_TILE).pow(2)
}
