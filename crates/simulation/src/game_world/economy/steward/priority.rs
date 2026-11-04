//! Kingdom reserves plus real deliveries; worker assignments are not income.
//! Each Steward's weights scale how strongly a resource's reserve counts.
use crate::GameWorld;
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
