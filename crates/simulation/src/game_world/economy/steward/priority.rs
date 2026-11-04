//! Kingdom reserves plus real deliveries; worker assignments are not income.
//! Each Steward's weights scale how strongly a resource's reserve counts.
use super::{STEWARD_RULES, StewardWork, rules::Budget, snapshot::Snapshot};
use crate::GameWorld;
use aoe_core::EntityId;
use aoe_core::PlayerId;
use aoe_map::ResourceKind;
use std::cmp::Ordering;

pub(super) const KINDS: [ResourceKind; 4] = [
    ResourceKind::Food,
    ResourceKind::Wood,
    ResourceKind::Gold,
    ResourceKind::Stone,
];

pub(super) struct Priority {
    reserves: [u64; 4],
    weights: [u64; 4],
}

impl Priority {
    /// Ordered by reserve per weight, then by higher weight, then by kind.
    /// Zero-weight kinds come last: they are used only when nothing else works.
    fn compare(&self, a: usize, b: usize) -> Ordering {
        let (wa, wb) = (self.weights[a], self.weights[b]);
        match (wa, wb) {
            (0, 0) => Ordering::Equal,
            (0, _) => Ordering::Greater,
            (_, 0) => Ordering::Less,
            _ => (u128::from(self.reserves[a]) * u128::from(wb))
                .cmp(&(u128::from(self.reserves[b]) * u128::from(wa)))
                .then(wb.cmp(&wa)),
        }
    }

    pub(super) fn kinds(&self) -> [ResourceKind; 4] {
        let mut kinds = KINDS;
        kinds.sort_by(|a, b| {
            let (a, b) = (*a as usize, *b as usize);
            self.compare(a, b).then(a.cmp(&b))
        });
        kinds
    }

    /// A productive worker changes only for a meaningful improvement. This
    /// margin does not affect idle jobs, so real small income differences count.
    pub(super) fn improves(&self, next: ResourceKind, current: ResourceKind) -> bool {
        self.improves_in_pass(next, current, false)
    }

    pub(super) fn improves_in_pass(
        &self,
        next: ResourceKind,
        current: ResourceKind,
        urgent_only: bool,
    ) -> bool {
        let (next, current) = (next as usize, current as usize);
        let margin = if urgent_only { 400_u128 } else { 105 };
        next != current
            && u128::from(self.reserves[next]) * u128::from(self.weights[current]) * margin
                < u128::from(self.reserves[current]) * u128::from(self.weights[next]) * 100
    }
}

impl GameWorld {
    /// Adjust one productive job per pass before observing the next deliveries.
    /// Fourfold shortages still dispatch the whole available workforce. Loaded
    /// workers deliver first; refused replacements preserve their existing work.
    pub(super) fn steward_rebalance(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(priority) = self.steward_priority(s.player, s.home.settings.weights) else {
            return;
        };
        let wanted = priority.kinds()[0];
        let after = s.home.worker_after;
        let split = after.map_or(0, |id| s.units.partition_point(|u| u.id <= id));
        let ids: Vec<_> = s.units[split..]
            .iter()
            .chain(&s.units[..split])
            .map(|u| u.id)
            .collect();
        for worker in ids {
            if budget.orders >= STEWARD_RULES.orders_per_domain {
                break;
            }
            if self.steward_controller(worker) != Some(s.steward)
                || self.economy.builders.contains_key(&worker)
                || self.economy.garrison_orders.contains_key(&worker)
                || self.unit(worker).is_none_or(|u| u.carried.is_some())
            {
                continue;
            }
            let Some(current) = self.gathering_kind(worker) else {
                continue;
            };
            if !priority.improves_in_pass(wanted, current, budget.reassigned) {
                continue;
            }
            if !budget.job() {
                break;
            }
            if let Some(home) = self.economy.steward_homes.get_mut(&s.steward) {
                home.worker_after = Some(worker)
            }
            s.avoid = self.economy.steward_homes[&s.steward]
                .stalls
                .get(&worker)
                .copied();
            let job = self.steward_job(worker, Some(current), s, budget, work);
            s.avoid = None;
            if let Some(kind) = job {
                budget.reassigned = true;
                self.claim_steward_worker(s.steward, worker, kind);
                changed.push(worker);
            }
        }
    }

    pub(super) fn steward_priority(&self, player: PlayerId, weights: [u8; 4]) -> Option<Priority> {
        let stock = self.player_economy(player)?.stockpile;
        let mut reserves = [stock.food, stock.wood, stock.gold, stock.stone].map(u64::from);
        if let Some(income) = self.economy.income.get(&player) {
            for (reserve, amount) in reserves
                .iter_mut()
                .zip(income.recent(self.tick.0, self.config.tick_hz))
            {
                *reserve = reserve.saturating_add(amount);
            }
        }
        Some(Priority {
            reserves,
            weights: weights.map(u64::from),
        })
    }
}
