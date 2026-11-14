//! Wounded-unit withdrawal; offensive groups own their combat acquisition.
use super::*;
use aoe_core::CombatStance;

#[cfg(test)]
#[path = "tests/withdrawal.rs"]
mod withdrawal_tests;

impl GameWorld {
    pub(super) fn general_fight(
        &mut self,
        u: GameUnit,
        ordered: &[GameUnit],
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let Some(a) = self.economy.generals.armies.get_mut(&u.id) else {
            return;
        };
        let home = a.home;
        let start = a.micro_after % ordered.len();
        a.micro_after = (start + 16).min(start + ordered.len()) % ordered.len();
        for n in 0..ordered.len().min(16) {
            let v = ordered[(start + n) % ordered.len()];
            work.orders += 1;
            if v.hp * 3 < v.max_hp {
                self.general_override_stance(
                    u.id,
                    v.id,
                    CombatStance::NoAttack,
                    stances::Reason::Wounded,
                );
                if let Ok(destination) = self.spread_destination(v.id, home)
                    && (v.attack_target.is_some()
                        || !self.moving_to_goal(
                            v.id,
                            destination,
                            crate::game_world::orders::GoalPrecision::Contact,
                        ))
                {
                    let _ = self.issue_contact_order(v.id, destination);
                }
            } else {
                continue;
            }
            changed.push(v.id);
        }
    }
}
