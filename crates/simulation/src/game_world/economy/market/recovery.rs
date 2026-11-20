use super::{MARKET_RULES, MarketPrices};
use aoe_core::PlayerId;
use aoe_map::ResourceKind;
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub(crate) enum MarketCommodity {
    Food,
    Wood,
    Stone,
}

impl MarketCommodity {
    fn from_resource(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Food => Self::Food,
            ResourceKind::Wood => Self::Wood,
            ResourceKind::Stone => Self::Stone,
            ResourceKind::Gold => unreachable!("trades validate the commodity"),
        }
    }

    pub(super) fn price(self, prices: &mut MarketPrices) -> &mut u16 {
        match self {
            Self::Food => &mut prices.food,
            Self::Wood => &mut prices.wood,
            Self::Stone => &mut prices.stone,
        }
    }
}

type CommodityKey = (PlayerId, MarketCommodity);

/// One deadline per off-base price. Idle markets do no per-player tick work.
#[derive(Debug, Default)]
pub(crate) struct MarketRecovery {
    due: BTreeSet<(u64, CommodityKey)>,
    by_commodity: BTreeMap<CommodityKey, u64>,
}

impl MarketRecovery {
    pub(super) fn after_trade(
        &mut self,
        player: PlayerId,
        kind: ResourceKind,
        price: u16,
        now: u64,
    ) {
        let key = (player, MarketCommodity::from_resource(kind));
        if price != MARKET_RULES.base_price {
            self.schedule(key, now + MARKET_RULES.recovery_ticks);
        } else if let Some(previous) = self.by_commodity.remove(&key) {
            self.due.remove(&(previous, key));
        }
    }

    pub(super) fn schedule(&mut self, key: CommodityKey, tick: u64) {
        if let Some(previous) = self.by_commodity.insert(key, tick) {
            self.due.remove(&(previous, key));
        }
        self.due.insert((tick, key));
    }

    pub(super) fn pop_due(&mut self, now: u64) -> Option<CommodityKey> {
        let &(tick, key) = self.due.first()?;
        if tick > now {
            return None;
        }
        self.due.pop_first();
        self.by_commodity.remove(&key);
        Some(key)
    }

    pub(crate) fn snapshot(&self, now: u64) -> Vec<(u64, PlayerId, MarketCommodity)> {
        self.due
            .iter()
            .map(|&(tick, (player, commodity))| (tick.saturating_sub(now), player, commodity))
            .collect()
    }

    pub(crate) fn restore(&mut self, now: u64, records: &[(u64, PlayerId, MarketCommodity)]) {
        *self = Self::default();
        for &(remaining_ticks, player, commodity) in records {
            self.schedule((player, commodity), now.saturating_add(remaining_ticks));
        }
    }

    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        if self.due.is_empty() {
            return;
        }
        hash.update(b"market-recovery");
        hash.update(&(self.due.len() as u64).to_le_bytes());
        for &(tick, (player, commodity)) in &self.due {
            hash.update(&tick.to_le_bytes());
            hash.update(&player.0.to_le_bytes());
            hash.update(&[commodity as u8]);
        }
    }
}
