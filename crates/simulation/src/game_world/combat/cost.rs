//! The hook a production building (Barracks) uses to train combat units.
use super::{UnitCost, UnitTraining, unit_combat};
use crate::{EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, WorldPosition};

impl GameWorld {
    /// Cost and build time of a unit, if a building can queue it through the
    /// combat table. Production buildings read this instead of hard-coding.
    pub fn unit_training(kind: UnitKind) -> Option<UnitTraining> {
        unit_combat(kind).training
    }

    /// Pay the full cost of `kind` or change nothing. Call when queueing.
    pub fn pay_unit_cost(&mut self, player: PlayerId, kind: UnitKind) -> Result<(), EconomyError> {
        let cost = Self::unit_training(kind)
            .ok_or(EconomyError::WrongUnitKind)?
            .cost;
        let state = self
            .economy
            .players
            .get_mut(&player)
            .ok_or(EconomyError::NotOwner)?;
        let stock = &mut state.stockpile;
        if stock.food < cost.food {
            return Err(EconomyError::InsufficientFood);
        }
        if stock.wood < cost.wood || stock.gold < cost.gold || stock.stone < cost.stone {
            return Err(EconomyError::InsufficientResources);
        }
        stock.food -= cost.food;
        stock.wood -= cost.wood;
        stock.gold -= cost.gold;
        stock.stone -= cost.stone;
        Ok(())
    }

    /// Return the cost paid by `pay_unit_cost`, for a cancelled queue entry.
    pub fn refund_unit_cost(&mut self, player: PlayerId, kind: UnitKind) {
        let UnitCost {
            food,
            wood,
            gold,
            stone,
        } = Self::unit_training(kind).map_or_else(UnitCost::default, |training| training.cost);
        if let Some(state) = self.economy.players.get_mut(&player) {
            let stock = &mut state.stockpile;
            stock.food = stock.food.saturating_add(food);
            stock.wood = stock.wood.saturating_add(wood);
            stock.gold = stock.gold.saturating_add(gold);
            stock.stone = stock.stone.saturating_add(stone);
        }
    }

    /// Spawns a finished militia; the Barracks calls it when a queue entry completes.
    pub fn spawn_militia(
        &mut self,
        player: PlayerId,
        position: WorldPosition,
    ) -> Result<EntityId, crate::GameWorldError> {
        let kind =
            UnitKind::Militia.upgraded(self.player_economy(player).map_or(0, |p| p.researched));
        self.spawn_kind(player, kind, position)
    }
}
