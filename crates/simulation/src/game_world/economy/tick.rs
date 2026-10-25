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
        let changed = self.advance_phases(sink, probe);
        // Every test tick checks that the route scheduler's counters equal
        // a recount: a drifted counter can block a player's routes for good.
        #[cfg(test)]
        if let Some(error) = self.route_accounting_error() {
            panic!("{error}");
        }
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
        probe(TickPhase::Movement);
        self.advance_spacing(&mut changed);
        probe(TickPhase::Spacing);
        self.advance_wildlife(&mut changed);
        probe(TickPhase::Wildlife);
        if self.economy.gatherers.is_empty()
            && self.combat.is_idle()
            && self.economy.builders.is_empty()
            && self.economy.buildings.is_empty()
            && self.wildlife.is_idle()
        {
            self.refill_route_budgets();
            if !changed.is_empty() {
                self.remember_visible_buildings();
            }
            return self.collect_changed(changed);
        }
        self.advance_combat(&mut changed);
        probe(TickPhase::Combat);
        self.advance_construction(&mut changed);
        self.advance_gates();
        probe(TickPhase::Construction);
        self.advance_gathering(sink, &mut changed);
        probe(TickPhase::Gathering);
        self.advance_research(&mut changed);
        self.advance_training(&mut changed);
        probe(TickPhase::ResearchTraining);
        self.remember_visible_buildings();
        probe(TickPhase::BuildingMemory);
        self.refill_route_budgets();
        let collected = self.collect_changed(changed);
        probe(TickPhase::Changed);
        collected
    }

    /// Every phase reports only the ids it touched. Sorting and deduping ids
    /// is far cheaper than sorting unit records, and one final fetch returns
    /// each changed unit's end-of-tick state in id order — the same output
    /// as when the phases pushed whole unit records. Economy may alter a
    /// mover a second time in the same tick; the fetch returns that final
    /// state, not the earlier movement interpolation state.
    fn collect_changed(&self, mut ids: Vec<EntityId>) -> Vec<GameUnit> {
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().filter_map(|id| self.unit(id)).collect()
    }
}
