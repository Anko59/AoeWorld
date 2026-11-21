//! An idle group shares what one member sees.
use super::super::{targeting::BuildingTargets, tick::Goal};
use super::autonomous_kind;
use crate::{GameWorld, UnitTask};
use aoe_core::{CombatStance, EntityId, WorldPosition};

impl GameWorld {
    /// When one idle member of `leader`'s group takes on an enemy, its idle
    /// mates look from where that member stands: otherwise only the edge of a
    /// block that sees enemies walking past engages them.
    pub(in crate::game_world::combat) fn alert_idle_group(
        &mut self,
        leader: EntityId,
        from: WorldPosition,
        changed: &mut Vec<EntityId>,
    ) {
        for mate in self.attached_members(leader) {
            let Some(u) = self.unit(mate).filter(|u| {
                u.hp > 0
                    && !u.auto_scout
                    && !matches!(u.stance, CombatStance::NoAttack | CombatStance::StandGround)
                    && matches!(u.task, UnitTask::Idle)
                    && !u.moving
                    && !u.planning
                    && autonomous_kind(u.kind)
                    && !self.embarked(u.id)
            }) else {
                continue;
            };
            let Some(mut area) = self.acquisition_area(&u) else {
                continue;
            };
            area.source = Goal::Unit(from, u.kind.collision_radius());
            if let Some(target) = self
                .acquire_target(&area, BuildingTargets::Unit)
                .target
                .filter(|&target| self.attack_spot_free(mate, target))
            {
                self.combat.quiet.wake(mate);
                self.begin_autonomous_attack(mate, target, u.position);
                changed.push(mate);
            }
        }
    }
}
