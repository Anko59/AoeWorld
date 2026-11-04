//! A fight splits the army into small groups; each clears its own enemy with
//! ordinary attack-move behaviour, and the regroup follows when none remains.
use super::{survey::Survey, *};
use aoe_core::CombatStance;

/// Soldiers per fighting group.
pub const FIGHT_GROUP: usize = 20;
/// Groups an army of the limit (and its General) can form.
pub const FIGHT_GROUPS: usize = (GENERAL_ARMY_LIMIT + 1).div_ceil(FIGHT_GROUP);

/// Siege, Monks, leaders, archers, then everyone else.
fn class(kind: UnitKind) -> u8 {
    match kind {
        k if role(k) == Some(3) => 0,
        UnitKind::Monk => 1,
        UnitKind::General | UnitKind::Vassal | UnitKind::Steward => 2,
        k if role(k) == Some(1) => 3,
        _ => 4,
    }
}

impl GameWorld {
    pub(super) fn general_fight(
        &mut self,
        u: GameUnit,
        ordered: &[GameUnit],
        survey: &Survey,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let building = survey.buildings.first().copied();
        let Some(a) = self.economy.generals.armies.get_mut(&u.id) else {
            return;
        };
        a.routed = None;
        let home = a.home;
        let start = a.micro_after % ordered.len();
        a.micro_after = (start + 16).min(start + ordered.len()) % ordered.len();
        // A rotating window keeps the wounded, the siege and a building-only
        // fight under the General's direct orders; the bounded cost per
        // decision does not grow with the army.
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
                let _ = self.issue_move(v.id, home);
            } else if role(v.kind) == Some(3) || survey.enemies.is_empty() {
                let siege = role(v.kind) == Some(3);
                let first = survey
                    .enemies
                    .iter()
                    .min_by_key(|v| {
                        (
                            class(v.kind),
                            u32::from(v.hp) * 1000 / u32::from(v.max_hp),
                            distance(v.position, u.position),
                            v.id,
                        )
                    })
                    .map(|v| v.id);
                if let Some(to) = if siege { building.or(first) } else { building }
                    && matches!(
                        self.issue_attack(u.player, v.id, to),
                        Err(crate::EconomyError::NoAttackPosition)
                    )
                {
                    self.resume_general_fighter(u.id, v.id, changed, work);
                }
            } else {
                continue;
            }
            changed.push(v.id);
        }
        if survey.enemies.is_empty() {
            return;
        }
        let fighters: Vec<_> = ordered
            .iter()
            .copied()
            .filter(|v| v.hp * 3 >= v.max_hp && role(v.kind) != Some(3))
            .collect();
        let squads: Vec<Vec<GameUnit>> = self.economy.generals.armies[&u.id]
            .squads
            .iter()
            .map(|squad| {
                squad
                    .members
                    .iter()
                    .filter_map(|id| fighters.iter().find(|v| v.id == *id).copied())
                    .collect()
            })
            .collect();
        // The four expensive firing-position searches are shared. Rotate the
        // first group by one each decision so unreachable early squads cannot
        // consume them forever. Include the commander in the same bounded ring.
        let count = squads.len() + 1;
        let Some(army) = self.economy.generals.armies.get_mut(&u.id) else {
            return;
        };
        let start = army.fight_after % count;
        army.fight_after = (start + 1) % count;
        for offset in 0..count {
            let group = (start + offset) % count;
            if group < squads.len() {
                if !squads[group].is_empty() {
                    self.general_fight_group(u.id, group, &squads[group], survey, changed, work);
                }
            } else if let Some(general) = fighters.iter().find(|v| v.id == u.id) {
                self.general_fight_group(
                    u.id,
                    squads::SQUAD_LIMIT,
                    &[*general],
                    survey,
                    changed,
                    work,
                );
            }
        }
    }

    fn general_fight_group(
        &mut self,
        g: EntityId,
        group: usize,
        soldiers: &[GameUnit],
        survey: &Survey,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let count = soldiers.len() as i64;
        let center = WorldPosition::new(
            (soldiers
                .iter()
                .map(|v| i64::from(v.position.x))
                .sum::<i64>()
                / count) as i32,
            (soldiers
                .iter()
                .map(|v| i64::from(v.position.y))
                .sum::<i64>()
                / count) as i32,
        );
        // Class, wounded fraction, distance from this group, then entity id.
        let key = |v: &GameUnit| {
            (
                class(v.kind),
                u32::from(v.hp) * 1000 / u32::from(v.max_hp),
                distance(v.position, center),
                v.id,
            )
        };
        let Some(best) = survey.enemies.iter().min_by_key(|v| key(v)) else {
            return;
        };
        let stored = self.economy.generals.armies[&g].fight[group];
        // The current target holds while it is visible and nothing of a
        // higher class appears; groups never trade targets back and forth.
        let kept = stored.and_then(|id| {
            survey
                .enemies
                .iter()
                .find(|e| e.id == id && class(e.kind) <= class(best.kind))
        });
        let target = kept.unwrap_or(best);
        let idle = soldiers
            .iter()
            .filter(|v| !v.moving && !v.planning && v.attack_target.is_none())
            .count()
            * 2
            > soldiers.len();
        // A retained group's route can fail after terrain closes. A surviving
        // moving/planning minority must not conceal that terminal failure.
        let failed = soldiers
            .iter()
            .any(|v| self.movement_failure(v.id).is_some());
        if kept.is_some() && !idle && !failed {
            return;
        }
        // A visible enemy's centre can be inside an unreachable enclosure.
        // Prove one firing position for the squad's longest-ranged soldier
        // through the same bounded search that ordinary soldiers use.
        let Some(representative) = soldiers.iter().max_by_key(|v| {
            let range = self
                .unit_stats(v.id)
                .map_or(0, |stats| crate::game_world::navy::attack_range(&stats));
            (
                range,
                std::cmp::Reverse(distance(v.position, target.position)),
                std::cmp::Reverse(v.id),
            )
        }) else {
            return;
        };
        if !self.reserve_attack_search() {
            return;
        }
        let Some(goal) = self.attack_goal(target.id, soldiers[0].player) else {
            return;
        };
        let Ok(search) = self.attack_search(representative.id, &goal) else {
            return;
        };
        let Some(destination) = search.destination else {
            if search.wait.is_none() {
                self.resume_general_squad(g, group, soldiers, changed, work);
                if let Some(army) = self.economy.generals.armies.get_mut(&g) {
                    army.fight[group] = None;
                }
            }
            return;
        };
        let ids: Vec<_> = soldiers.iter().map(|v| v.id).collect();
        work.orders += 1;
        let ordered = self.issue_formation_move(
            soldiers[0].player,
            &ids,
            aoe_core::Formation::Line,
            destination,
            false,
            true,
        );
        if ordered.is_err() {
            // No shared route: each soldier attacks alone, bounded by the group.
            for v in soldiers {
                work.orders += 1;
                let _ = self.issue_attack(v.player, v.id, target.id);
            }
        }
        if let Some(a) = self.economy.generals.armies.get_mut(&g) {
            a.fight[group] = Some(target.id);
        }
        changed.extend(ids);
    }

    /// The enemy a fighting group is committed to (tests and the HUD).
    pub fn general_fight_target(&self, g: EntityId, group: usize) -> Option<EntityId> {
        self.economy
            .generals
            .armies
            .get(&g)?
            .fight
            .get(group)
            .copied()
            .flatten()
    }
}
