use super::{survey::Survey, *};
use aoe_core::CombatStance;
fn strength(u: GameUnit) -> u64 {
    let s = crate::unit_combat(u.kind);
    u64::from(u.hp) * u64::from(s.attack.max(1)) * 200
        / u64::from(s.attack_period_centiseconds.max(1))
}
impl GameWorld {
    pub(super) fn general_decide(
        &mut self,
        u: GameUnit,
        survey: Survey,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let members: Vec<_> = self.economy.generals.armies[&u.id]
            .members
            .iter()
            .copied()
            .collect();
        let mut army: Vec<_> = members
            .iter()
            .filter_map(|&id| self.unit(id))
            .filter(|v| v.hp > 0 && !self.embarked(v.id))
            .collect();
        army.push(u);
        let friendly: u64 = army
            .iter()
            .copied()
            .filter(|v| distance(u.position, v.position) <= (16 * 1024_i64).pow(2))
            .map(strength)
            .sum();
        let enemy: u64 = survey.enemies.iter().copied().map(strength).sum::<u64>()
            + survey
                .buildings
                .iter()
                .filter_map(|&id| self.building(id))
                .map(|b| {
                    if matches!(
                        b.kind,
                        crate::BuildingKind::Castle | crate::BuildingKind::WatchTower
                    ) {
                        2500
                    } else {
                        0
                    }
                })
                .sum::<u64>();
        let Some(a) = self.economy.generals.armies.get_mut(&u.id) else {
            return;
        };
        let old_state = a.state;
        let hurt = u.hp * 3 < u.max_hp
            || army.iter().map(|v| u64::from(v.hp)).sum::<u64>() * 3
                < army.iter().map(|v| u64::from(v.max_hp)).sum::<u64>();
        let unsafe_fight = !survey.complete || enemy * 4 > friendly * 5 || (enemy > 0 && hurt);
        if unsafe_fight {
            a.retreat_until = self
                .tick
                .0
                .saturating_add(u64::from(self.config.tick_hz) * 5);
        }
        a.state = if unsafe_fight || self.tick.0 < a.retreat_until {
            GeneralState::Retreating
        } else if !survey.enemies.is_empty() || !survey.buildings.is_empty() {
            GeneralState::Fighting
        } else {
            GeneralState::Marching
        };
        let state = a.state;
        let destination = if state == GeneralState::Retreating {
            Some(a.home)
        } else {
            a.destination
        };
        if old_state != state {
            a.routed = None;
        }
        if state != GeneralState::Fighting {
            if let Some(to) = destination {
                self.general_formation(
                    u.id,
                    &army,
                    to,
                    state == GeneralState::Retreating,
                    changed,
                    work,
                );
            }
            return;
        }
        // Stable high-value / wounded target order; ties always break by id.
        let target = survey
            .enemies
            .iter()
            .min_by_key(|v| {
                let valuable = matches!(
                    v.kind,
                    UnitKind::Mangonel
                        | UnitKind::Monk
                        | UnitKind::General
                        | UnitKind::Archer
                        | UnitKind::Crossbowman
                );
                (
                    u8::from(!valuable),
                    u32::from(v.hp) * 1000 / u32::from(v.max_hp),
                    distance(v.position, u.position),
                    v.id,
                )
            })
            .map(|v| v.id);
        let building = survey.buildings.first().copied();
        let Some(a) = self.economy.generals.armies.get_mut(&u.id) else {
            return;
        };
        a.routed = None;
        let start = a.micro_after % army.len();
        a.micro_after = (start + 16).min(start + army.len()) % army.len();
        for n in 0..army.len().min(16) {
            let v = army[(start + n) % army.len()];
            work.orders += 1;
            if v.hp * 3 < v.max_hp {
                let home = self.economy.generals.armies[&u.id].home;
                let _ = self.issue_stance(u.player, &[v.id], CombatStance::NoAttack);
                let _ = self.issue_move(v.id, home);
            } else {
                let _ = self.issue_stance(u.player, &[v.id], CombatStance::Aggressive);
                let siege = matches!(v.kind, UnitKind::Mangonel | UnitKind::BatteringRam);
                if let Some(to) = if siege {
                    building.or(target)
                } else {
                    target.or(building)
                } {
                    let _ = self.issue_attack(u.player, v.id, to);
                }
            }
            changed.push(v.id);
        }
    }
}
