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

#[derive(Debug, Default)]
pub(crate) struct OrderDiagnostics {
    pub issuer: OrderIssuer,
    pub demand: [OrderDemand; 7],
    last: BTreeMap<EntityId, (u64, OrderIssuer)>,
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

    pub(crate) fn forget_order_demand(&mut self, id: EntityId) {
        self.order_diagnostics.last.remove(&id);
    }

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
    }

    pub(crate) fn demand_cancelled(&mut self, id: EntityId) {
        if self.movement_goal(id).is_some()
            && let Some((tick, issuer)) = self.order_diagnostics.last.get(&id).copied()
        {
            let row = &mut self.order_diagnostics.demand[issuer as usize];
            row.superseded += 1;
            row.superseded_10s +=
                u64::from(self.tick.0.saturating_sub(tick) < 10 * u64::from(self.config.tick_hz));
        }
    }

    pub(crate) fn demand_accepted(&mut self, id: EntityId) {
        let unfinished = self.movement_goal(id).is_some();
        let issuer = self.order_diagnostics.issuer;
        if let Some((tick, previous)) = self
            .order_diagnostics
            .last
            .insert(id, (self.tick.0, issuer))
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
            }
        }
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
