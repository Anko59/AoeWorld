//! FARM produces food independently of the farmer's work rate. Accrue lazily
//! on harvest attempts, including time spent carrying or without a farmer.
use crate::GameWorld;
use aoe_core::EntityId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FarmProduction {
    credits: u64,
    last_tick: u64,
}

impl FarmProduction {
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(&self.credits.to_le_bytes());
        hash.update(&self.last_tick.to_le_bytes());
    }
}

impl GameWorld {
    pub(crate) fn reset_farm_production(&mut self, farm: EntityId) {
        self.economy.farm_production.insert(
            farm,
            FarmProduction {
                credits: 0,
                last_tick: self.tick.0,
            },
        );
    }

    /// Take one produced food; None means not yet produced or no live farm.
    pub(crate) fn deplete_farm(&mut self, farm: EntityId) -> Option<u16> {
        let remaining = self
            .building(farm)
            .filter(|b| b.is_complete())?
            .food_remaining?;
        if remaining == 0 {
            return None;
        }
        let threshold = u64::from(self.config.tick_hz) * 1_000_000;
        let production = self.economy.farm_production.get_mut(&farm)?;
        let elapsed = self.tick.0.saturating_sub(production.last_tick);
        let rate = u64::from(super::FARM_PRODUCTION_HUNDREDTHS) * 10_000;
        production.credits = production
            .credits
            .saturating_add(elapsed.saturating_mul(rate))
            .min(u64::from(remaining.min(super::FARM_STORAGE_CAPACITY)) * threshold);
        production.last_tick = self.tick.0;
        if production.credits < threshold {
            return None;
        }
        production.credits -= threshold;
        let food = self
            .economy
            .buildings
            .get_mut(&farm)?
            .food_remaining
            .as_mut()?;
        *food -= 1;
        Some(*food)
    }
}
