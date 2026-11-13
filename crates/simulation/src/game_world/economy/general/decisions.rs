use super::{survey::Survey, *};
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
        // MoveAlone suspends army leadership until an army march or Stop.
        // In particular, a new sighting must not start a fight or retreat.
        if self.economy.generals.armies[&u.id].solo {
            return;
        }
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
        let ordered: Vec<_> = army
            .iter()
            .copied()
            .filter(|v| in_influence(u.position, v.position) || self.formation(v.id).is_some())
            .collect();
        // The whole army counts, wherever its soldiers stand: a column strung
        // out along the road is as strong as it is when it gathers.
        let friendly: u64 = army.iter().copied().map(strength).sum();
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
        let completed = army.iter().all(|v| {
            !v.moving
                && !v.planning
                && v.attack_target.is_none()
                && !self.combat.resuming.contains_key(&v.id)
                && self.movement_goal(v.id).is_none()
        });
        let Some(a) = self.economy.generals.armies.get_mut(&u.id) else {
            return;
        };
        let old_state = a.state;
        let hurt = u.hp * 3 < u.max_hp
            || army.iter().map(|v| u64::from(v.hp)).sum::<u64>() * 3
                < army.iter().map(|v| u64::from(v.max_hp)).sum::<u64>();
        let unsafe_fight = enemy * 4 > friendly * 5 || (enemy > 0 && hurt);
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
        } else if a.destination.is_none() {
            GeneralState::Idle
        } else {
            GeneralState::Marching
        };
        let state = a.state;
        // A finished fight with no march order regroups around the General.
        let regroup = (old_state == GeneralState::Fighting
            && state != GeneralState::Fighting
            && a.destination.is_none())
        .then_some(u.position);
        // A finished ordinary click may have passed a visible contact. Only
        // after every admitted march/fight finishes, make another offensive
        // click; never replace an underway order with firing-position micro.
        let engagement = if state == GeneralState::Fighting && completed {
            survey.enemies.first().map(|enemy| enemy.position)
        } else {
            (old_state == GeneralState::Fighting && state == GeneralState::Fighting)
                .then_some(a.layout)
                .flatten()
                .map(|(goal, _)| goal)
        };
        let destination = if state == GeneralState::Retreating {
            Some(a.home)
        } else {
            a.destination.or(regroup)
        };
        if (old_state == GeneralState::Retreating) != (state == GeneralState::Retreating) {
            a.routed = None;
            a.layout = None;
            a.commander_march = None;
            for squad in &mut a.squads {
                squad.march = None;
            }
        }
        self.reconcile_general_stances(u.id, state);
        if ordered.is_empty() {
            return;
        }
        if let Some(to) = engagement
            .filter(|_| state != GeneralState::Retreating)
            .or(destination)
            .or_else(|| survey.enemies.first().map(|v| v.position))
        {
            let healthy: Vec<_> = ordered
                .iter()
                .copied()
                .filter(|v| state != GeneralState::Fighting || v.hp * 3 >= v.max_hp)
                .collect();
            self.march_squads(
                u.id,
                &healthy,
                to,
                state == GeneralState::Retreating,
                changed,
                work,
            );
        }
        if state == GeneralState::Fighting {
            self.general_fight(u, &ordered, changed, work);
        }
    }
}
