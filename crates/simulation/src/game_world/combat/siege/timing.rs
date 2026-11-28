//! DAT attack release frame multiplied by its graphic's cell duration.
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, Tick};

pub const fn native_attack_windup_millis(kind: UnitKind) -> Option<u64> {
    Some(match kind {
        UnitKind::Mangonel
        | UnitKind::Onager
        | UnitKind::SiegeOnager
        | UnitKind::BatteringRam
        | UnitKind::CappedRam
        | UnitKind::SiegeRam
        | UnitKind::CatapultGalleon => 0,
        UnitKind::Scorpion | UnitKind::HeavyScorpion => 160,
        UnitKind::BombardCannon => 210,
        UnitKind::HandCannoneer => 350,
        UnitKind::CavalryArcher => 910,
        UnitKind::HeavyCavalryArcher => 897,
        UnitKind::CamelRider | UnitKind::HeavyCamelRider => 500,
        UnitKind::TrebuchetUnpacked => 880,
        _ => return None,
    })
}

impl GameWorld {
    pub(in crate::game_world::combat) fn reset_native_swing(
        &mut self,
        id: EntityId,
        kind: UnitKind,
        changed: &mut Vec<EntityId>,
    ) {
        if native_attack_windup_millis(kind).is_some()
            && let Some(attack) = self.combat.attackers.get_mut(&id)
            && attack.swing_started != 0
        {
            attack.swing_started = 0;
            changed.push(id);
        }
    }

    pub fn native_attack_started(&self, id: EntityId) -> Option<Tick> {
        let started = self.combat.attackers.get(&id)?.swing_started;
        (started != 0).then_some(Tick(started))
    }

    pub(in crate::game_world::combat) fn weapon_ready(
        &mut self,
        index: usize,
        changed: &mut Vec<EntityId>,
    ) -> bool {
        let unit = &self.units[index];
        let now = self.tick.0;
        let ready = self.weapon_ready_tick(index);
        let Some(delay) = native_attack_windup_millis(unit.state.kind) else {
            return now >= ready;
        };
        let delay = (delay * u64::from(self.config.tick_hz)).div_ceil(1_000);
        let id = unit.state.id;
        let Some(attack) = self.combat.attackers.get_mut(&id) else {
            return false;
        };
        if attack.swing_started == 0 || attack.swing_started.saturating_add(delay) < ready {
            if now.saturating_add(delay) < ready {
                return false;
            }
            attack.swing_started = now;
            changed.push(id);
        }
        now >= ready.max(attack.swing_started.saturating_add(delay))
    }
}
