use crate::{GameWorld, UnitKind};
use aoe_core::PlayerId;

const QUANTA_PER_SECOND: u64 = 1_000_000;
const QUANTA_PER_FOOD: u64 = 60 * QUANTA_PER_SECOND;

impl UnitKind {
    /// Units supplied at one food per game minute, including Scouts, Monks
    /// and Generals. Garrisoning and embarking leave the bill unchanged.
    pub const fn needs_food_upkeep(self) -> bool {
        !matches!(
            self,
            Self::Villager
                | Self::FishingShip
                | Self::TransportShip
                | Self::Steward
                | Self::Vassal
                | Self::TradeCart
                | Self::TradeCog
        )
    }
}

impl GameWorld {
    /// The current food bill per game minute and whether the army is unsupplied.
    pub fn food_upkeep(&self, player: PlayerId) -> (u32, bool) {
        let Some(state) = self.economy.players.get(&player) else {
            return (0, false);
        };
        let rate = self.economy.upkeep_units.get(&player).copied().unwrap_or(0);
        (
            rate,
            rate > 0 && state.upkeep_shortage && state.stockpile.food == 0,
        )
    }

    pub(super) fn charge_food_upkeep(&mut self) {
        if self.economy.players.is_empty() {
            return;
        }
        let hz = u64::from(self.config.tick_hz);
        let phase = (self.tick.0 - 1) % hz;
        // Adjacent endpoints partition each second exactly at any tick rate.
        // Carry is stored in these clock-independent quanta across saves.
        let elapsed = (phase + 1) * QUANTA_PER_SECOND / hz - phase * QUANTA_PER_SECOND / hz;
        for (player, state) in &mut self.economy.players {
            let units = self.economy.upkeep_units.get(player).copied().unwrap_or(0);
            if units == 0 {
                state.upkeep_shortage = false;
                continue;
            }
            let due = u64::from(state.upkeep_remainder) + u64::from(units) * elapsed;
            let bill = (due / QUANTA_PER_FOOD) as u32;
            state.upkeep_remainder = (due % QUANTA_PER_FOOD) as u32;
            let food = state.stockpile.food;
            // Forgive the unpaid whole portion. Fresh food lifts the shortage
            // immediately, and each subsequent tick collects only its own bill.
            state.stockpile.food = food.saturating_sub(bill);
            state.upkeep_shortage = bill > food || (food == 0 && state.upkeep_shortage);
        }
    }
}
