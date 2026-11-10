use crate::{GameUnit, GameWorld, PreparedResourceDepletion};
use aoe_core::EntityId;

/// A phase of one authoritative tick, reported to `advance_with_probe`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickPhase {
    Movement,
    Spacing,
    Wildlife,
    Combat,
    Construction,
    Gathering,
    ResearchTraining,
    StewardEconomy,
    BuildingMemory,
    Changed,
}

impl GameWorld {
    pub fn advance(&mut self) -> Vec<GameUnit> {
        self.advance_with_resource_sink(&mut |_| true)
    }

    /// The adapter accepts a validated resource transaction before simulation
    /// commits it. A refused transaction earns no carried resource.
    pub fn advance_with_resource_sink(
        &mut self,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
    ) -> Vec<GameUnit> {
        self.advance_with_probe(sink, &mut |_| {})
    }

    /// Same tick as `advance_with_resource_sink`; `probe` is called as each
    /// phase finishes so a caller can attribute time without a clock here.
    pub fn advance_with_probe(
        &mut self,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
        probe: &mut impl FnMut(TickPhase),
    ) -> Vec<GameUnit> {
        #[cfg(test)]
        {
            let units = &self.units;
            self.collisions
                .resync_bodies(|index| units[index].state.position);
        }
        self.economy.stewards.automatic = true;
        let changed = self.advance_phases(sink, probe);
        self.publish_unit_changes(&changed);
        self.recover_market_prices();
        self.economy.stewards.automatic = false;
        // Every test tick checks that the route scheduler's counters equal
        // a recount: a drifted counter can block a player's routes for good.
        #[cfg(test)]
        if let Some(error) = self.route_accounting_error() {
            panic!("{error}");
        }
        // Sweeps reject by the index's copy of each centre, not the unit's.
        #[cfg(test)]
        self.collisions
            .assert_bodies(|index| self.units[index].state.position);
        changed
    }

    fn advance_phases(
        &mut self,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
        probe: &mut impl FnMut(TickPhase),
    ) -> Vec<GameUnit> {
        // A consumer has one tick to drain these bounded changes. Keeping an
        // undrained history would grow storage without a transport obligation.
        self.economy.depletions.clear();
        let mut changed = self.advance_movement();
        self.with_order_issuer(crate::OrderIssuer::AutoScout, |w| {
            w.advance_auto_scout(&mut changed)
        });
        probe(TickPhase::Movement);
        self.advance_transports(&mut changed);
        self.advance_spacing(&mut changed);
        probe(TickPhase::Spacing);
        self.advance_wildlife(&mut changed);
        probe(TickPhase::Wildlife);
        if self.economy.gatherers.is_empty()
            && self.combat.is_idle()
            && self.economy.builders.is_empty()
            && self.economy.garrison_orders.is_empty()
            && self.economy.ejections.is_empty()
            && self.economy.buildings.is_empty()
            && self.wildlife.is_idle()
            && self.economy.generals.armies.is_empty()
        {
            self.refill_route_budgets();
            if !changed.is_empty() {
                self.remember_visible_buildings();
            }
            return self.collect_changed(changed);
        }
        self.advance_garrisons(&mut changed);
        self.advance_combat(&mut changed);
        probe(TickPhase::Combat);
        self.advance_construction(&mut changed);
        self.advance_gates();
        probe(TickPhase::Construction);
        let gathered = changed.len();
        self.advance_gathering(sink, &mut changed);
        probe(TickPhase::Gathering);
        self.advance_research(&mut changed);
        self.advance_training(&mut changed);
        probe(TickPhase::ResearchTraining);
        self.with_order_issuer(crate::OrderIssuer::Vassal, |w| {
            w.advance_vassals(&mut changed)
        });
        self.with_order_issuer(crate::OrderIssuer::General, |w| {
            w.advance_generals(&mut changed)
        });
        self.with_order_issuer(crate::OrderIssuer::Steward, |w| {
            w.advance_stewards(&mut changed)
        });
        probe(TickPhase::StewardEconomy);
        self.remember_visible_buildings();
        probe(TickPhase::BuildingMemory);
        self.refill_route_budgets();
        self.economy.late_changed = changed[gathered..].to_vec();
        let collected = self.collect_changed(changed);
        probe(TickPhase::Changed);
        collected
    }

    /// Every phase reports only the ids it touched. Their dirty bits dedupe
    /// them and read back in id order, with no sort, and one final fetch
    /// returns each changed unit's end-of-tick state — the same output as
    /// when the phases pushed whole unit records. Economy may alter a mover a
    /// second time in the same tick; the fetch returns that final state, not
    /// the earlier movement interpolation state.
    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    fn collect_changed(&mut self, ids: Vec<EntityId>) -> Vec<GameUnit> {
        self.passes
            .record(crate::game_world::Pass::CollectChanged, ids.len());
        let mut dirty = std::mem::take(&mut self.movement_scratch.changed);
        for &id in &ids {
            dirty.insert(id);
        }
        let changed = dirty.iter().filter_map(|id| self.unit(id)).collect();
        dirty.clear();
        self.movement_scratch.changed = dirty;
        changed
    }
}

#[cfg(test)]
mod tests;
