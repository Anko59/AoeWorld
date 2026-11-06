//! AoE II DE Market commodity trading: one per-player gold price per
//! resource, moved by the player's own trades. The Genie dat carries no
//! trading fields (the bounded surveys in `aoe-harness assets dat-table` and
//! the SiegeEngineers extractions cited by docs/economy/feudal.md expose
//! neither prices nor the Guilds fee), so the constants are DE's documented
//! engine rules: prices start at 100 gold per 100 units, buying costs the
//! price plus a 30% commission, selling pays the price minus 30%, every trade
//! moves the price 3 gold, and prices stay within [20, 9999]. Between trades
//! every price drifts back toward the starting price by one gold each
//! `recovery_ticks`, so a market worn down by a large order keeps working.
use super::{BuildingKind, EconomyError};
use crate::GameWorld;
use aoe_core::{EntityId, MARKET_MAX_PRICE, MARKET_MIN_PRICE, PlayerId};
use aoe_map::ResourceKind;

/// The single current market rules table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarketRules {
    /// Units per trade; prices are quoted per this amount.
    pub trade_amount: u32,
    /// Starting price in gold per trade.
    pub base_price: u16,
    pub min_price: u16,
    pub max_price: u16,
    /// Commission in ten-thousandths; 3000 is DE's 30%.
    pub commission_permyriad: u16,
    /// Gold the price moves on every trade.
    pub price_step: u16,
    /// Ticks between recovery steps (two seconds at the default 20 Hz).
    pub recovery_ticks: u64,
}
pub const MARKET_RULES: MarketRules = MarketRules {
    trade_amount: 100,
    base_price: 100,
    min_price: MARKET_MIN_PRICE,
    max_price: MARKET_MAX_PRICE,
    commission_permyriad: 3000,
    price_step: 3,
    recovery_ticks: 40,
};
impl MarketRules {
    /// Buying 100 units costs the price plus the commission.
    pub fn buy_cost(&self, price: u16) -> u32 {
        u32::from(price) * u32::from(10_000 + self.commission_permyriad) / 10_000
    }
    /// Selling 100 units pays the price minus the commission.
    pub fn sell_proceeds(&self, price: u16) -> u32 {
        u32::from(price) * u32::from(10_000 - self.commission_permyriad) / 10_000
    }
    pub fn raise(&self, price: u16) -> u16 {
        price.saturating_add(self.price_step).min(self.max_price)
    }
    pub fn lower(&self, price: u16) -> u16 {
        price.saturating_sub(self.price_step).max(self.min_price)
    }
    /// One recovery step toward the starting price, never past it.
    pub fn recover(&self, price: u16) -> u16 {
        match price.cmp(&self.base_price) {
            std::cmp::Ordering::Less => price + 1,
            std::cmp::Ordering::Equal => price,
            std::cmp::Ordering::Greater => price - 1,
        }
    }
}

/// The player's commodity prices, in gold per 100 units.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MarketPrices {
    pub wood: u16,
    pub food: u16,
    pub stone: u16,
}
impl Default for MarketPrices {
    fn default() -> Self {
        Self {
            wood: MARKET_RULES.base_price,
            food: MARKET_RULES.base_price,
            stone: MARKET_RULES.base_price,
        }
    }
}
impl MarketPrices {
    /// `None` for gold: gold is the market's currency, not a commodity.
    pub fn price(&self, kind: ResourceKind) -> Option<u16> {
        match kind {
            ResourceKind::Wood => Some(self.wood),
            ResourceKind::Food => Some(self.food),
            ResourceKind::Stone => Some(self.stone),
            ResourceKind::Gold => None,
        }
    }
    pub fn set(&mut self, kind: ResourceKind, price: u16) {
        match kind {
            ResourceKind::Wood => self.wood = price,
            ResourceKind::Food => self.food = price,
            ResourceKind::Stone => self.stone = price,
            ResourceKind::Gold => {}
        }
    }
}

impl super::Stockpile {
    /// Adds to the matching stockpile field; the market never depletes.
    pub(crate) fn market_add(&mut self, kind: ResourceKind, amount: u32) {
        let field = match kind {
            ResourceKind::Food => &mut self.food,
            ResourceKind::Wood => &mut self.wood,
            ResourceKind::Gold => &mut self.gold,
            ResourceKind::Stone => &mut self.stone,
        };
        *field = field.saturating_add(amount);
    }
    /// Removes `amount` of `kind`, or reports the shortfall.
    pub(crate) fn market_take(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let field = match kind {
            ResourceKind::Food => &mut self.food,
            ResourceKind::Wood => &mut self.wood,
            ResourceKind::Gold => &mut self.gold,
            ResourceKind::Stone => &mut self.stone,
        };
        if *field < amount {
            return false;
        }
        *field -= amount;
        true
    }
}

impl GameWorld {
    /// Called once per tick: prices step toward the base price every
    /// `recovery_ticks`, independently per player and commodity.
    pub(crate) fn recover_market_prices(&mut self) {
        if !self.tick.0.is_multiple_of(MARKET_RULES.recovery_ticks) {
            return;
        }
        for state in self.economy.players.values_mut() {
            let p = &mut state.market_prices;
            p.wood = MARKET_RULES.recover(p.wood);
            p.food = MARKET_RULES.recover(p.food);
            p.stone = MARKET_RULES.recover(p.stone);
        }
    }

    /// Buy 100 units of `kind` for gold at the player's current price plus
    /// commission; then that price rises by the step, within the clamp.
    pub fn issue_market_buy(
        &mut self,
        player: PlayerId,
        market: EntityId,
        kind: ResourceKind,
    ) -> Result<(), EconomyError> {
        let market_kind = self.market_kind(player, market)?;
        if market_kind != BuildingKind::Market {
            return Err(EconomyError::NoMarket);
        }
        let price = self
            .economy
            .players
            .get(&player)
            .and_then(|state| state.market_prices.price(kind))
            .ok_or(EconomyError::InvalidResource)?;
        let cost = MARKET_RULES.buy_cost(price);
        let Some(state) = self.economy.players.get_mut(&player) else {
            return Err(EconomyError::NotOwner);
        };
        if state.stockpile.gold < cost {
            return Err(EconomyError::InsufficientResources);
        }
        // No fallible work follows these mutations.
        state.stockpile.gold -= cost;
        state.stockpile.market_add(kind, MARKET_RULES.trade_amount);
        state.market_prices.set(kind, MARKET_RULES.raise(price));
        Ok(())
    }

    /// Sell 100 units of `kind` for gold at the player's current price minus
    /// commission; then that price falls by the step, within the clamp.
    pub fn issue_market_sell(
        &mut self,
        player: PlayerId,
        market: EntityId,
        kind: ResourceKind,
    ) -> Result<(), EconomyError> {
        let market_kind = self.market_kind(player, market)?;
        if market_kind != BuildingKind::Market {
            return Err(EconomyError::NoMarket);
        }
        let price = self
            .economy
            .players
            .get(&player)
            .and_then(|state| state.market_prices.price(kind))
            .ok_or(EconomyError::InvalidResource)?;
        let proceeds = MARKET_RULES.sell_proceeds(price);
        let Some(state) = self.economy.players.get_mut(&player) else {
            return Err(EconomyError::NotOwner);
        };
        if !state.stockpile.market_take(kind, MARKET_RULES.trade_amount) {
            return Err(EconomyError::InsufficientResources);
        }
        // No fallible work follows these mutations.
        state.stockpile.gold = state.stockpile.gold.saturating_add(proceeds);
        state.market_prices.set(kind, MARKET_RULES.lower(price));
        Ok(())
    }
    /// Up to `lots` consecutive 100-unit trades. This is exactly what pressing
    /// the single-trade button `lots` times would do: every lot is priced at
    /// the price the previous lot left (so a large order walks the price down
    /// or up by `price_step` per lot until the clamp), and the order stops at
    /// the first lot the stockpile cannot pay for. It returns how many lots
    /// executed; none executed returns the reason the first lot failed.
    pub fn issue_market_trade(
        &mut self,
        player: PlayerId,
        market: EntityId,
        kind: ResourceKind,
        buy: bool,
        lots: u32,
    ) -> Result<u32, EconomyError> {
        let market_kind = self.market_kind(player, market)?;
        if market_kind != BuildingKind::Market {
            return Err(EconomyError::NoMarket);
        }
        let Some(state) = self.economy.players.get_mut(&player) else {
            return Err(EconomyError::NotOwner);
        };
        let mut price = state
            .market_prices
            .price(kind)
            .ok_or(EconomyError::InvalidResource)?;
        let mut done = 0;
        while done < lots {
            if buy {
                let cost = MARKET_RULES.buy_cost(price);
                if state.stockpile.gold < cost {
                    break;
                }
                state.stockpile.gold -= cost;
                state.stockpile.market_add(kind, MARKET_RULES.trade_amount);
                price = MARKET_RULES.raise(price);
            } else {
                if !state.stockpile.market_take(kind, MARKET_RULES.trade_amount) {
                    break;
                }
                let proceeds = MARKET_RULES.sell_proceeds(price);
                state.stockpile.gold = state.stockpile.gold.saturating_add(proceeds);
                price = MARKET_RULES.lower(price);
            }
            done += 1;
        }
        if done == 0 {
            return Err(EconomyError::InsufficientResources);
        }
        state.market_prices.set(kind, price);
        Ok(done)
    }
    /// The completed building a trade names; ownership and completion first.
    fn market_kind(&self, player: PlayerId, id: EntityId) -> Result<BuildingKind, EconomyError> {
        let building = self
            .economy
            .buildings
            .get(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !building.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        Ok(building.kind)
    }
}
