//! Kingdom reserves plus real deliveries; worker assignments are not income.
//! Each Steward's weights scale how strongly a resource's reserve counts.
use super::snapshot::Snapshot;
use crate::{BuildingKind, GameWorld};
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

    /// `next` is so much scarcer than `current` (under a tenth of its
    /// weighted reserve) that a busy worker should change over to it.
    pub(super) fn severe(&self, next: ResourceKind, current: ResourceKind) -> bool {
        let (next, current) = (next as usize, current as usize);
        next != current
            && self.weights[next] > 0
            && u128::from(self.reserves[next]) * u128::from(self.weights[current]) * 10
                < u128::from(self.reserves[current]) * u128::from(self.weights[next])
    }

    /// Discovery can prepare a meaningfully more needed kind before jobs end.
    pub(super) fn improves(&self, next: ResourceKind, current: ResourceKind) -> bool {
        let (next, current) = (next as usize, current as usize);
        next != current
            && u128::from(self.reserves[next]) * u128::from(self.weights[current]) * 105
                < u128::from(self.reserves[current]) * u128::from(self.weights[next]) * 100
    }
}

impl GameWorld {
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

impl GameWorld {
    /// What a job search of this domain can find, as one number: the census
    /// state, resource order, which buildings the stockpile pays for, the
    /// housing need, the known buildings (finished or not, free paid Farms),
    /// the discovered nodes and the managed head count. A failed search is
    /// repeated only when this changes: a node discovered, a building placed,
    /// finished or lost, a Farm freed, a zone change, a resource affordable.
    pub(super) fn steward_offer(&self, s: &Snapshot) -> u64 {
        let mix = |h: u64, v: u64| (h ^ v).wrapping_mul(0x0100_0000_01b3).rotate_left(17);
        let mut h = mix(0xcbf2_9ce4_8422_2325, u64::from(s.complete));
        if let Some(priority) = self.steward_priority(s.player, s.home.settings.weights) {
            for kind in priority.kinds() {
                h = mix(h, kind as u64);
            }
        }
        if let Some(p) = self.player_economy(s.player) {
            let a = p.stockpile;
            for kind in [
                BuildingKind::House,
                BuildingKind::Mill,
                BuildingKind::LumberCamp,
                BuildingKind::MiningCamp,
                BuildingKind::Farm,
            ] {
                let c = kind.rules().cost;
                let paid =
                    a.food >= c.food && a.wood >= c.wood && a.gold >= c.gold && a.stone >= c.stone;
                h = mix(h, u64::from(paid));
            }
            let housing =
                p.population_used as usize + s.global_queued + 2 >= p.population_cap as usize;
            h = mix(h, u64::from(housing));
        }
        // Order-independent: a building or node joining or leaving changes
        // the sum, a rotation of the same queue does not.
        let mut set = 0u64;
        for b in &s.buildings {
            let free_farm = b.kind == BuildingKind::Farm
                && b.food_remaining.is_some_and(|food| food > 0)
                && self
                    .economy
                    .gatherers
                    .count_target(super::super::gathering::WorkTarget::Farm(b.id))
                    == 0;
            let key =
                u64::from(b.id.0) << 8 | u64::from(b.is_complete()) << 1 | u64::from(free_farm);
            set = set.wrapping_add(mix(0x9e37_79b9_7f4a_7c15, key));
        }
        h = mix(h, set);
        h = mix(h, s.home.resources.candidate_digest());
        mix(
            h,
            self.economy.steward_homes[&s.steward].workers.len() as u64,
        )
    }
}
