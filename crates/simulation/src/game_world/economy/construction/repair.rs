//! Repairing a completed building: integer HP work priced at half its cost.
use super::super::Stockpile;
use crate::GameWorld;
use aoe_core::EntityId;

impl GameWorld {
    /// Integer repair work, priced at half the building's original cost for
    /// a full HP bar. HP thresholds retain fractional resource costs without
    /// introducing a clock or an unbounded per-builder accumulator.
    pub(super) fn repair_building(&mut self, id: EntityId, builders: u64) {
        let Some(b) = self.economy.buildings.get(&id) else {
            return;
        };
        let hp = u64::from(b.hp);
        let max = u64::from(b.max_hp);
        let next = (hp + ((builders + 2) * max / b.work_total.max(1)).max(1)).min(max);
        let cost = b.construction_cost();
        let charge = |amount| {
            ((u64::from(amount) * next / (2 * max)) - (u64::from(amount) * hp / (2 * max))) as u32
        };
        let Some(player) = self.economy.players.get_mut(&b.player) else {
            return;
        };
        let s = &mut player.stockpile;
        let bill = Stockpile {
            food: charge(cost.food),
            wood: charge(cost.wood),
            gold: charge(cost.gold),
            stone: charge(cost.stone),
        };
        if s.food < bill.food || s.wood < bill.wood || s.gold < bill.gold || s.stone < bill.stone {
            return;
        }
        s.food -= bill.food;
        s.wood -= bill.wood;
        s.gold -= bill.gold;
        s.stone -= bill.stone;
        if let Some(building) = self.economy.buildings.get_mut(&id) {
            building.hp = next as u16;
        }
        if next == max {
            self.combat.hp_quarters[id.0 as usize] = 0;
        }
    }
}
