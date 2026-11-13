//! Bounded typed offices inside an army. Each squad owns one ordinary shared
//! route; all squads consume the General's single bounded threat census.
use super::*;

#[cfg(test)]
#[path = "tests/march_churn.rs"]
mod march_churn_tests;

pub(super) const SQUAD_LIMIT: usize = 10;
const CAPACITY: [usize; 4] = [20, 15, 10, 5];

#[derive(Debug)]
pub(crate) struct Squad {
    pub arm: usize,
    pub members: Vec<EntityId>,
    /// Retained objective and the projected final ordinary formation goal.
    pub march: Option<(WorldPosition, WorldPosition)>,
    pub final_leg: bool,
}

pub(super) fn arm(kind: UnitKind) -> Option<usize> {
    role(kind).map(|role| if role == 4 { 2 } else { role })
}

impl Army {
    /// Include prepaid soldiers before admitting another body. Reservations
    /// fill existing offices first, then consume one of the ten free offices.
    pub(crate) fn room_for(&self, kind: UnitKind) -> bool {
        let mut offices: Vec<_> = self
            .squads
            .iter()
            .map(|s| (s.arm, s.members.len()))
            .collect();
        for kind in self.pending.values().copied().chain([kind]) {
            let Some(arm) = arm(kind) else { return false };
            if let Some((_, size)) = offices
                .iter_mut()
                .find(|(a, size)| *a == arm && *size < CAPACITY[arm])
            {
                *size += 1;
            } else if offices.len() < SQUAD_LIMIT {
                offices.push((arm, 1));
            } else {
                return false;
            }
        }
        true
    }

    pub(super) fn assign_squad(
        &mut self,
        id: EntityId,
        kind: UnitKind,
    ) -> Option<(Option<EntityId>, EntityId)> {
        let arm = arm(kind)?;
        if let Some(squad) = self
            .squads
            .iter_mut()
            .find(|s| s.arm == arm && s.members.len() < CAPACITY[arm])
        {
            let old = squad.members.first().copied();
            if let Err(at) = squad.members.binary_search(&id) {
                squad.members.insert(at, id);
            }
            let new = *squad.members.first()?;
            (old != Some(new)).then_some((old, new))
        } else {
            debug_assert!(self.squads.len() < SQUAD_LIMIT);
            self.squads.push(Squad {
                arm,
                members: vec![id],
                march: None,
                final_leg: false,
            });
            Some((None, id))
        }
    }

    pub(super) fn remove_from_squads(&mut self, id: EntityId) -> Option<EntityId> {
        let mut promoted = None;
        for squad in &mut self.squads {
            let leader = squad.members.first() == Some(&id);
            squad.members.retain(|&member| member != id);
            if leader {
                promoted = squad.members.first().copied();
            }
        }
        self.squads.retain(|squad| !squad.members.is_empty());
        promoted
    }
}

fn offset(arm: usize, number: usize, counts: &[usize; 4], scout: bool) -> WorldPosition {
    let width = counts[arm].clamp(1, 4);
    let infantry_rows = counts[0].div_ceil(4).max(1) as i32;
    let archer_rows = counts[1].div_ceil(4).max(1) as i32;
    let side = (2 * (number % width) as i32 - (width as i32 - 1)) * 4;
    let row = (number / width) as i32 * 8;
    let (side, along) = match arm {
        0 => (side, 8 - row),
        1 => (side, 4 - infantry_rows * 8 - row),
        2 => (
            if number.is_multiple_of(2) { -20 } else { 20 },
            (if scout { 16 } else { 8 }) - (number / 2) as i32 * 8,
        ),
        _ => (side, 8 - (infantry_rows + archer_rows) * 8 - row),
    };
    WorldPosition::new(side * 1024, along * 1024)
}

struct Leg<'a> {
    squad: Option<usize>,
    ids: &'a [EntityId],
    objective: WorldPosition,
    desired: WorldPosition,
    finish: WorldPosition,
    final_leg: bool,
    retreat: bool,
    pace: (u64, u64),
}

impl GameWorld {
    /// The slowest selection leads through its ordinary route. Other squads,
    /// including the commander, take short offensive legs along that route.
    pub(super) fn march_squads(
        &mut self,
        g: EntityId,
        army: &[GameUnit],
        to: WorldPosition,
        retreat: bool,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let Some(general) = self.unit(g) else { return };
        let available: BTreeSet<_> = army.iter().map(|u| u.id).collect();
        let heading = self.economy.generals.armies[&g]
            .layout
            .map(|(_, h)| h)
            .unwrap_or(WorldPosition::new(
                to.x - general.position.x,
                to.y - general.position.y,
            ));
        if let Some(army) = self.economy.generals.armies.get_mut(&g) {
            army.layout = Some((to, heading));
        }
        let mut counts = [0; 4];
        for squad in &self.economy.generals.armies[&g].squads {
            counts[squad.arm] += 1;
        }
        let mut numbers = [0; 4];
        let mut plans: Vec<_> = self.economy.generals.armies[&g]
            .squads
            .iter()
            .enumerate()
            .filter_map(|(at, squad)| {
                let number = numbers[squad.arm];
                numbers[squad.arm] += 1;
                let ids: Vec<_> = squad
                    .members
                    .iter()
                    .copied()
                    .filter(|id| available.contains(id))
                    .collect();
                if ids.is_empty() {
                    return None;
                }
                let scout = ids
                    .iter()
                    .any(|&id| self.unit(id).is_some_and(|u| role(u.kind) == Some(4)));
                Some((Some(at), ids, offset(squad.arm, number, &counts, scout)))
            })
            .collect();
        if available.contains(&g) {
            plans.push((None, vec![g], WorldPosition::new(0, 0)));
        }
        let Some((guide, pace)) = plans
            .iter()
            .enumerate()
            .filter_map(|(at, (_, ids, _))| {
                ids.iter()
                    .filter_map(|&id| self.unit(id))
                    .map(|u| self.movement_speed(u.kind, u.player))
                    .min_by(|a, b| {
                        (u128::from(a.0) * u128::from(b.1))
                            .cmp(&(u128::from(b.0) * u128::from(a.1)))
                    })
                    .map(|speed| (at, speed))
            })
            .min_by(|a, b| {
                (u128::from(a.1.0) * u128::from(b.1.1))
                    .cmp(&(u128::from(b.1.0) * u128::from(a.1.1)))
            })
        else {
            return;
        };
        let guide_goal = self.general_offset_point(to, heading, plans[guide].2);
        self.admit_general_leg(
            g,
            Leg {
                squad: plans[guide].0,
                ids: &plans[guide].1,
                objective: to,
                desired: guide_goal,
                finish: guide_goal,
                final_leg: true,
                retreat,
                pace,
            },
            changed,
            work,
        );
        let Some((checkpoint, route_heading)) = plans[guide]
            .1
            .iter()
            .find_map(|&id| self.movement_checkpoint(id, 64 * 1024))
        else {
            // A pending route has no waypoint yet. Sending the rest to the
            // guide's current position would march advanced bodies backwards.
            return;
        };
        let guide_army = &self.economy.generals.armies[&g];
        let guide_final = plans[guide]
            .0
            .map_or(guide_army.commander_march, |at| guide_army.squads[at].march)
            .map_or(guide_goal, |(_, goal)| goal);
        let guide_route_final = plans[guide].0.map_or(guide_army.routed == Some(to), |at| {
            guide_army.squads[at].final_leg
        });
        let final_leg = guide_route_final && checkpoint == guide_final;
        for (at, (squad, ids, offset)) in plans.iter().enumerate() {
            if at == guide {
                continue;
            }
            // Intermediate legs stay in a compact band around the shared
            // corridor; the wide battle layout belongs only at the objective.
            let relative =
                WorldPosition::new(offset.x.clamp(-4096, 4096), offset.y.clamp(-4096, 4096));
            let final_goal = self.general_offset_point(to, heading, *offset);
            let near_objective = ids.iter().all(|&id| {
                self.unit(id).is_some_and(|u| {
                    (i64::from(u.position.x - final_goal.x).pow(2)
                        + i64::from(u.position.y - final_goal.y).pow(2))
                        <= (64_i64 * 1024).pow(2)
                })
            });
            let final_leg = final_leg || near_objective;
            let goal = if final_leg {
                self.general_offset_point(to, heading, *offset)
            } else {
                self.general_offset_point(checkpoint, route_heading, relative)
            };
            self.admit_general_leg(
                g,
                Leg {
                    squad: *squad,
                    ids,
                    objective: to,
                    desired: goal,
                    finish: final_goal,
                    final_leg,
                    retreat,
                    pace,
                },
                changed,
                work,
            );
        }
    }

    fn general_offset_point(
        &self,
        centre: WorldPosition,
        heading: WorldPosition,
        offset: WorldPosition,
    ) -> WorldPosition {
        let length = (i64::from(heading.x).pow(2) + i64::from(heading.y).pow(2))
            .isqrt()
            .max(1);
        self.config.clamp_ground_position(WorldPosition::new(
            centre.x
                + ((i64::from(heading.x) * i64::from(offset.y)
                    - i64::from(heading.y) * i64::from(offset.x))
                    / length) as i32,
            centre.y
                + ((i64::from(heading.y) * i64::from(offset.y)
                    + i64::from(heading.x) * i64::from(offset.x))
                    / length) as i32,
        ))
    }

    fn admit_general_leg(
        &mut self,
        g: EntityId,
        leg: Leg<'_>,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let Leg {
            squad,
            ids,
            objective,
            desired,
            finish,
            final_leg,
            retreat,
            pace,
        } = leg;
        let army = &self.economy.generals.armies[&g];
        let admitted = squad.map_or(army.commander_march, |at| army.squads[at].march);
        let same = admitted.is_some_and(|(to, _)| to == objective);
        let saved_final = same
            && admitted.is_some_and(|(_, finish)| {
                ids.iter().any(|&id| self.movement_goal(id) == Some(finish))
            });
        if saved_final && let Some(army) = self.economy.generals.armies.get_mut(&g) {
            if let Some(at) = squad {
                army.squads[at].final_leg = true;
            } else {
                army.routed = Some(objective);
            }
        }
        let active = ids.iter().any(|&id| {
            self.unit(id).is_some_and(|u| {
                u.moving
                    || u.planning
                    || u.attack_target.is_some()
                    || self.combat.resuming.contains_key(&id)
            })
        });
        let final_leg = if same {
            saved_final || !active
        } else {
            final_leg
        };
        let general = self.unit(g).unwrap();
        let dx = i64::from(objective.x - general.position.x);
        let dy = i64::from(objective.y - general.position.y);
        let length = (dx * dx + dy * dy).isqrt();
        let ahead = length >= 1024
            && ids.iter().any(|&id| {
                self.unit(id).is_some_and(|u| {
                    i64::from(u.position.x - general.position.x) * dx
                        + i64::from(u.position.y - general.position.y) * dy
                        > length * 12 * 1024
                })
            });
        let pace_limit = (!final_leg || squad.is_none() || ahead).then_some(pace);
        if retreat {
            for &id in ids {
                self.general_override_stance(
                    g,
                    id,
                    aoe_core::CombatStance::Defensive,
                    stances::Reason::Retreat,
                );
            }
        }
        for &id in ids {
            if self.formation(id).is_some() {
                let _ = self.issue_formation_pace(general.player, id, pace_limit);
            }
        }
        let occupied = |w: &Self, id| {
            w.formation(id).is_some()
                || w.unit(id).is_some_and(|u| {
                    u.moving
                        || u.planning
                        || u.attack_target.is_some()
                        || w.combat.resuming.contains_key(&id)
                })
        };
        if same && active && ids.iter().all(|&id| occupied(self, id)) {
            return;
        }
        // Reinforcements join the admitted click. If combat consumed a queued
        // continuation, a completed cohort gets its final ordinary click next.
        let desired = if same && active {
            ids.iter()
                .find_map(|&id| self.movement_goal(id))
                .unwrap_or(finish)
        } else if same {
            finish
        } else {
            desired
        };
        let Some(player) = self.unit(ids[0]).map(|u| u.player) else {
            return;
        };
        let Ok((_, centre)) = self.formation_members(player, ids) else {
            return;
        };
        let Ok(origin) = self.formation_origin(ids, centre) else {
            return;
        };
        let resolve = |point| {
            self.formation_goal(
                player,
                origin,
                self.config.snap_ground_position(point),
                aoe_core::Formation::Line,
                ids,
            )
        };
        let Ok(goal) = resolve(desired) else {
            return;
        };
        let Ok(finish) = resolve(finish) else {
            return;
        };
        let retained =
            same && (active || (saved_final && admitted.is_some_and(|(_, old)| old == goal)));
        let pending: Vec<_> = ids
            .iter()
            .copied()
            .filter(|&id| !(retained && occupied(self, id)))
            .collect();
        if !pending.is_empty() && !self.formation_matches(ids, goal) {
            work.orders += 1;
            if self
                .issue_formation_move(
                    player,
                    &pending,
                    aoe_core::Formation::Line,
                    goal,
                    false,
                    true,
                )
                .is_ok()
            {
                if !final_leg && goal != finish {
                    // The shared FIFO continues the anchor through a bend;
                    // rebuilding a physical centroid on a deck can put it off-lane.
                    work.orders += 1;
                    let _ = self.issue_formation_move(
                        player,
                        &pending,
                        aoe_core::Formation::Line,
                        finish,
                        true,
                        true,
                    );
                }
                let _ = self.issue_formation_pace(player, pending[0], pace_limit);
                if let Some(army) = self.economy.generals.armies.get_mut(&g) {
                    if let Some(at) = squad {
                        army.squads[at].march = Some((objective, finish));
                        army.squads[at].final_leg = final_leg;
                    } else {
                        army.commander_march = Some((objective, finish));
                        army.routed = final_leg.then_some(objective);
                    }
                }
                changed.extend(&pending);
            }
        }
    }
}
