use crate::{GameUnit, GameWorld, PreparedResourceDepletion};

/// A phase of one authoritative tick, reported to `advance_with_probe`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickPhase {
    Movement,
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
        // A consumer has one tick to drain these bounded changes. Keeping an
        // undrained history would grow storage without a transport obligation.
        self.economy.depletions.clear();
        let mut changed = self.advance_movement();
        probe(TickPhase::Movement);
        self.advance_wildlife(&mut changed);
        probe(TickPhase::Wildlife);
        if self.economy.gatherers.is_empty()
            && self.combat.is_idle()
            && self.economy.builders.is_empty()
            && self.economy.buildings.is_empty()
            && self.wildlife.is_idle()
        {
            self.planning_budget = crate::MAX_ROUTE_WORK_PER_TICK;
            if !changed.is_empty() {
                self.remember_visible_buildings();
            }
            return changed;
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
        self.planning_budget = crate::MAX_ROUTE_WORK_PER_TICK;
        changed.sort_by_key(|unit| unit.id);
        changed.dedup_by_key(|unit| unit.id);
        changed.retain(|unit| self.unit_exists(unit.id));
        // Economy may alter a mover a second time in the same tick. Return its
        // final state, not the earlier movement interpolation state.
        for unit in &mut changed {
            if let Some(current) = self.unit(unit.id) {
                *unit = current;
            }
        }
        probe(TickPhase::Changed);
        changed
    }
}
