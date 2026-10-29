use super::*;
use aoe_core::CombatStance;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Reason {
    March,
    Retreat,
    Wounded,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct StanceOverride {
    pub id: EntityId,
    pub original: CombatStance,
    pub reason: Reason,
}

impl GameWorld {
    pub(super) fn general_override_stance(
        &mut self,
        g: EntityId,
        id: EntityId,
        stance: CombatStance,
        reason: Reason,
    ) {
        let Some(&index) = self.lookup.get(&id) else {
            return;
        };
        let original = self.units[index].state.stance;
        let Some(a) = self.economy.generals.armies.get_mut(&g) else {
            return;
        };
        if let Some(saved) = a.stances.iter_mut().flatten().find(|s| s.id == id) {
            saved.reason = reason;
        } else {
            // One inline slot per soldier plus the General; never allocate in a decision.
            let Some(slot) = a.stances.iter_mut().find(|s| s.is_none()) else {
                return;
            };
            *slot = Some(StanceOverride {
                id,
                original,
                reason,
            });
        }
        // The accompanying formation/move order cancels the previous attack.
        self.units[index].state.stance = stance;
    }

    fn restore_stance(&mut self, saved: StanceOverride) {
        if let Some(&index) = self.lookup.get(&saved.id) {
            self.units[index].state.stance = saved.original;
        }
    }

    pub(crate) fn restore_general_stance(&mut self, g: EntityId, id: EntityId) {
        let saved = self.economy.generals.armies.get_mut(&g).and_then(|a| {
            a.stances
                .iter_mut()
                .find(|s| s.is_some_and(|s| s.id == id))?
                .take()
        });
        if let Some(saved) = saved {
            self.restore_stance(saved);
        }
    }

    pub(super) fn restore_general_stances(&mut self, g: EntityId) {
        let Some(a) = self.economy.generals.armies.get_mut(&g) else {
            return;
        };
        let saved = std::mem::replace(&mut a.stances, [None; GENERAL_ARMY_LIMIT + 1]);
        for stance in saved.into_iter().flatten() {
            self.restore_stance(stance);
        }
    }

    pub(super) fn reconcile_general_stances(
        &mut self,
        g: EntityId,
        state: GeneralState,
        destination: Option<WorldPosition>,
    ) {
        // Fixed stack snapshot permits restoration without allocating or borrowing the army.
        let saved = self.economy.generals.armies[&g].stances;
        for (slot, saved) in saved.into_iter().enumerate() {
            let Some(saved) = saved else { continue };
            let keep = self.unit(saved.id).is_some_and(|u| {
                u.hp > 0
                    && !self.embarked(u.id)
                    && match saved.reason {
                        Reason::March => {
                            state == GeneralState::Marching
                                && destination.is_some()
                                && (u.moving || u.planning)
                        }
                        Reason::Retreat => state == GeneralState::Retreating,
                        Reason::Wounded => state == GeneralState::Fighting && u.hp * 3 < u.max_hp,
                    }
            });
            if !keep {
                if let Some(a) = self.economy.generals.armies.get_mut(&g) {
                    a.stances[slot] = None;
                }
                self.restore_stance(saved);
            }
        }
    }
}
