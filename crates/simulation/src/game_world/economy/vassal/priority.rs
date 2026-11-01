//! Kingdom reserves plus real deliveries; worker assignments are not income.
use super::{VASSAL_RULES, VassalWork, rules::Budget, snapshot::Snapshot};
use crate::GameWorld;
use aoe_core::EntityId;
use aoe_core::PlayerId;
use aoe_map::ResourceKind;

const MIX: [u64; 4] = [45, 30, 15, 10];
pub(super) const KINDS: [ResourceKind; 4] = [
    ResourceKind::Food,
    ResourceKind::Wood,
    ResourceKind::Gold,
    ResourceKind::Stone,
];

pub(super) struct Priority {
    reserves: [u64; 4],
}

impl Priority {
    pub(super) fn kinds(&self) -> [ResourceKind; 4] {
        let mut kinds = KINDS;
        kinds.sort_by(|a, b| {
            let (a, b) = (*a as usize, *b as usize);
            (u128::from(self.reserves[a]) * u128::from(MIX[b]))
                .cmp(&(u128::from(self.reserves[b]) * u128::from(MIX[a])))
                .then(a.cmp(&b))
        });
        kinds
    }

    /// A productive worker changes only for a meaningful improvement. This
    /// margin does not affect idle jobs, so real small income differences count.
    pub(super) fn improves(&self, next: ResourceKind, current: ResourceKind) -> bool {
        let (next, current) = (next as usize, current as usize);
        next != current
            && u128::from(self.reserves[next]) * u128::from(MIX[current]) * 105
                < u128::from(self.reserves[current]) * u128::from(MIX[next]) * 100
    }
}

impl GameWorld {
    /// Retask productive gatherers only after delivery. Ordinary command
    /// admission preserves their existing work when a replacement is refused.
    pub(super) fn vassal_rebalance(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(priority) = self.vassal_priority(s.player) else {
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
            if budget.orders >= VASSAL_RULES.orders_per_domain
                || budget.searches >= VASSAL_RULES.resource_searches_per_domain
            {
                break;
            }
            if self.vassal_controller(worker) != Some(s.vassal)
                || self.economy.builders.contains_key(&worker)
                || self.economy.garrison_orders.contains_key(&worker)
                || self.unit(worker).is_none_or(|u| u.carried.is_some())
            {
                continue;
            }
            let Some(current) = self.gathering_kind(worker) else {
                continue;
            };
            if !priority.improves(wanted, current) {
                continue;
            }
            if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                home.worker_after = Some(worker)
            }
            s.avoid = self.economy.vassal_homes[&s.vassal]
                .stalls
                .get(&worker)
                .copied();
            let job = self.vassal_job(worker, Some(current), s, budget, work);
            s.avoid = None;
            if let Some(kind) = job {
                self.claim_vassal_worker(s.vassal, worker, kind);
                changed.push(worker);
            }
        }
    }

    pub(super) fn vassal_priority(&self, player: PlayerId) -> Option<Priority> {
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
        Some(Priority { reserves })
    }
}
