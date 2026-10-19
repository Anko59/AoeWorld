use crate::{GameUnit, GameWorld, PreparedResourceDepletion};

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
        // A consumer has one tick to drain these bounded changes. Keeping an
        // undrained history would grow storage without a transport obligation.
        self.economy.depletions.clear();
        let mut changed = self.advance_movement();
        if self.economy.gatherers.is_empty()
            && self.combat.is_idle()
            && self.economy.builders.is_empty()
            && self.economy.buildings.is_empty()
        {
            self.planning_budget = crate::MAX_ROUTE_WORK_PER_TICK;
            return changed;
        }
        self.advance_combat(&mut changed);
        self.advance_construction(&mut changed);
        self.advance_gathering(sink, &mut changed);
        self.advance_research(&mut changed);
        self.advance_training(&mut changed);
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
        changed
    }
}
