//! Bounded typed offices inside an army. Each squad owns one ordinary shared
//! route; all squads consume the General's single bounded threat census.
use super::*;

pub(super) const SQUAD_LIMIT: usize = 10;
const CAPACITY: [usize; 4] = [20, 15, 10, 5];

#[derive(Debug)]
pub(crate) struct Squad {
    pub arm: usize,
    pub members: Vec<EntityId>,
    /// The destination this squad was refused and when. A refusal is not
    /// completion, but it is retried only after `REFUSED_RETRY_TICKS`.
    pub refused: Option<(WorldPosition, u64)>,
}

/// Ticks before a squad refused a march is ordered again: ground may have
/// opened, yet a General must not reissue a refusal at every decision.
const REFUSED_RETRY_TICKS: u64 = 256;

pub(super) fn arm(kind: UnitKind) -> Option<usize> {
    role(kind).map(|role| if role == 4 { 2 } else { role })
}

impl Army {
    /// Include prepaid soldiers before admitting another body. Reservations
    /// fill existing offices first, then consume one of the ten free offices.
    pub(super) fn room_for(&self, kind: UnitKind) -> bool {
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
                refused: None,
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

impl GameWorld {
    pub(crate) fn general_squad_pace(&self, id: EntityId) -> Option<(u64, u64)> {
        let g = self.general_controller(id).unwrap_or(id);
        let army = self.economy.generals.armies.get(&g)?;
        if !matches!(
            army.state,
            GeneralState::Marching | GeneralState::Retreating
        ) {
            return None;
        }
        if id == g {
            return Some(army.pace);
        }
        let general = self.unit(g)?;
        let unit = self.unit(id)?;
        let to = if army.state == GeneralState::Retreating {
            army.home
        } else {
            army.destination?
        };
        let (dx, dy) = (
            i64::from(to.x) - i64::from(general.position.x),
            i64::from(to.y) - i64::from(general.position.y),
        );
        let length = (dx * dx + dy * dy).isqrt();
        let lead = (i64::from(unit.position.x) - i64::from(general.position.x)) * dx
            + (i64::from(unit.position.y) - i64::from(general.position.y)) * dy;
        // Independent squads catch up at their own speed. A twelve-tile lead
        // keeps scouts loosely ahead without letting them abandon the march.
        // Once the commander arrives, each squad finishes its local approach.
        (length >= 1024 && lead > length * 12 * 1024).then_some(army.pace)
    }

    fn squad_destination(
        &self,
        kind: UnitKind,
        to: WorldPosition,
        heading: WorldPosition,
        offset: WorldPosition,
    ) -> Option<WorldPosition> {
        let length = (i64::from(heading.x).pow(2) + i64::from(heading.y).pow(2))
            .isqrt()
            .max(1);
        let desired = WorldPosition::new(
            to.x + ((i64::from(heading.x) * i64::from(offset.y)
                - i64::from(heading.y) * i64::from(offset.x))
                / length) as i32,
            to.y + ((i64::from(heading.y) * i64::from(offset.y)
                + i64::from(heading.x) * i64::from(offset.x))
                / length) as i32,
        );
        let desired = self.squad_ground_towards(kind, to, desired);
        // A bounded shared centre search, once per squad order. Ordinary
        // formation assignment separately validates each physical final slot.
        std::iter::once(desired)
            .chain((1_i32..=8).flat_map(|ring| {
                (-ring..=ring).flat_map(move |y| {
                    (-ring..=ring).filter_map(move |x| {
                        (x.abs().max(y.abs()) == ring).then_some(WorldPosition::new(
                            desired.x + x * 1024,
                            desired.y + y * 1024,
                        ))
                    })
                })
            }))
            .find(|&point| self.squad_ground(kind, point))
    }

    /// A squad's goal stands on ground an ordinary formation order accepts.
    fn squad_ground(&self, kind: UnitKind, point: WorldPosition) -> bool {
        self.config.valid_ground_position(point)
            && self.body_position_clear(kind, point)
            && !self.economy.blocked_tiles.contains(&point.tile_floor())
            && [kind, UnitKind::Militia].into_iter().all(|kind| {
                self.unit_tile_passable(kind, point.tile_floor(), &|| false)
                    .unwrap_or(false)
            })
    }

    /// Walks from the General's goal toward a squad's place in the army and
    /// stops at the first water, tree or wall. A flank twenty tiles wide of a
    /// goal on a river bank stays on that bank instead of aiming at the water
    /// or at a far shore reached only by a long way round.
    fn squad_ground_towards(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
    ) -> WorldPosition {
        let (dx, dy) = (
            i64::from(to.x) - i64::from(from.x),
            i64::from(to.y) - i64::from(from.y),
        );
        let steps = (dx.abs().max(dy.abs()) / 1024).clamp(1, 64);
        let mut last = from;
        for step in 1..=steps {
            let next = WorldPosition::new(
                from.x + (dx * step / steps) as i32,
                from.y + (dy * step / steps) as i32,
            );
            if !self.squad_ground(kind, next) {
                break;
            }
            last = next;
        }
        self.config.snap_ground_position(last)
    }

    /// Fight rejection can precede the first march decision. Establish the
    /// same stable heading for both admission and recovery.
    fn general_march_heading(&mut self, g: EntityId, to: WorldPosition) -> Option<WorldPosition> {
        let general = self.unit(g)?;
        let army = self.economy.generals.armies.get_mut(&g)?;
        if let Some((goal, heading)) = army.layout
            && goal == to
        {
            return Some(heading);
        }
        let mut heading = WorldPosition::new(to.x - general.position.x, to.y - general.position.y);
        if heading == WorldPosition::new(0, 0) {
            heading = WorldPosition::new(1024, 0);
        }
        army.layout = Some((to, heading));
        Some(heading)
    }

    /// Direct siege/building orders also retain the army's march when their
    /// firing-position search proves the target unreachable.
    pub(super) fn resume_general_fighter(
        &mut self,
        g: EntityId,
        id: EntityId,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let army = &self.economy.generals.armies[&g];
        let group = if id == g {
            SQUAD_LIMIT
        } else {
            let Some(group) = army.squads.iter().position(|s| s.members.contains(&id)) else {
                return;
            };
            group
        };
        let soldiers: Vec<_> = if group == SQUAD_LIMIT {
            self.unit(g).into_iter().collect()
        } else {
            army.squads[group]
                .members
                .iter()
                .filter_map(|&id| self.unit(id))
                .collect()
        };
        if !soldiers.is_empty() {
            self.resume_general_squad(g, group, &soldiers, changed, work);
        }
    }

    pub(super) fn resume_general_squad(
        &mut self,
        g: EntityId,
        group: usize,
        soldiers: &[GameUnit],
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let Some(to) = self.economy.generals.armies[&g].destination else {
            return;
        };
        let Some(heading) = self.general_march_heading(g, to) else {
            return;
        };
        let army = &self.economy.generals.armies[&g];
        let goal = if group == SQUAD_LIMIT {
            Some(to)
        } else {
            let squad = &army.squads[group];
            let mut counts = [0; 4];
            for squad in &army.squads {
                counts[squad.arm] += 1;
            }
            let number = army.squads[..group]
                .iter()
                .filter(|s| s.arm == squad.arm)
                .count();
            let scout = soldiers.iter().any(|v| role(v.kind) == Some(4));
            let Some(largest) = soldiers
                .iter()
                .map(|v| v.kind)
                .max_by_key(|k| k.collision_radius())
            else {
                return;
            };
            self.squad_destination(
                largest,
                to,
                heading,
                offset(squad.arm, number, &counts, scout),
            )
        };
        let Some(goal) = goal else {
            return;
        };
        let ids: Vec<_> = soldiers.iter().map(|v| v.id).collect();
        if !self.formation_matches(&ids, goal) {
            work.orders += 1;
            if self
                .issue_formation_move(
                    soldiers[0].player,
                    &ids,
                    aoe_core::Formation::Line,
                    goal,
                    false,
                    false,
                )
                .is_ok()
            {
                changed.extend(ids);
            }
        }
    }

    pub(super) fn march_squads(
        &mut self,
        g: EntityId,
        army: &[GameUnit],
        to: WorldPosition,
        retreat: bool,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        if self.economy.generals.armies[&g].routed == Some(to) {
            return;
        }
        let Some(general) = self.unit(g) else { return };
        let Some(heading) = self.general_march_heading(g, to) else {
            return;
        };
        let available: BTreeSet<_> = army.iter().map(|u| u.id).collect();
        let mut counts = [0; 4];
        for squad in &self.economy.generals.armies[&g].squads {
            counts[squad.arm] += 1;
        }
        let mut numbers = [0; 4];
        let mut plans = Vec::new();
        for (at, squad) in self.economy.generals.armies[&g].squads.iter().enumerate() {
            let number = numbers[squad.arm];
            numbers[squad.arm] += 1;
            let ids: Vec<_> = squad
                .members
                .iter()
                .copied()
                .filter(|id| available.contains(id))
                .collect();
            if ids.is_empty() {
                continue;
            }
            let scout = ids
                .iter()
                .any(|&id| self.unit(id).is_some_and(|u| role(u.kind) == Some(4)));
            let Some(largest) = ids
                .iter()
                .filter_map(|&id| self.unit(id))
                .map(|u| u.kind)
                .max_by_key(|kind| kind.collision_radius())
            else {
                continue;
            };
            let goal = self.squad_destination(
                largest,
                to,
                heading,
                offset(squad.arm, number, &counts, scout),
            );
            plans.push((if scout { 0 } else { squad.arm + 1 }, ids, goal, Some(at)));
        }
        // Leading cavalry gets the destination's first look. The ordinary
        // observer/fog system reports sightings through the shared survey.
        plans.sort_by_key(|(priority, ids, ..)| (*priority, ids[0]));
        plans.push((5, vec![g], Some(to), None));
        let marching = self.economy.generals.armies[&g].state != GeneralState::Idle;
        let mut complete = true;
        let now = self.tick.0;
        for (_, ids, goal, squad) in plans {
            let Some(goal) = goal else {
                complete = false;
                continue;
            };
            let refused = squad
                .and_then(|at| self.economy.generals.armies[&g].squads[at].refused)
                .is_some_and(|(refused_to, tick)| {
                    refused_to == to && now.saturating_sub(tick) < REFUSED_RETRY_TICKS
                });
            if refused {
                complete = false;
                continue;
            }
            if !self.formation_matches(&ids, goal) {
                work.orders += 1;
                let order = self.issue_formation_move(
                    general.player,
                    &ids,
                    aoe_core::Formation::Line,
                    goal,
                    false,
                    false,
                );
                if order.is_err() {
                    // A refusal is not completion: ground may open later, so
                    // the march stays unrouted. The squad is asked again only
                    // after an interval, never at every decision while its
                    // soldiers stand and wait.
                    complete = false;
                    if let Some(at) = squad
                        && !matches!(
                            order,
                            Err(crate::EconomyError::Movement(
                                crate::GameWorldError::PathBudgetExceeded
                            ))
                        )
                        && let Some(army) = self.economy.generals.armies.get_mut(&g)
                    {
                        army.squads[at].refused = Some((to, now));
                    }
                    continue;
                }
            }
            if marching {
                for &id in &ids {
                    let (stance, reason) = if retreat {
                        (aoe_core::CombatStance::NoAttack, stances::Reason::Retreat)
                    } else {
                        (aoe_core::CombatStance::Defensive, stances::Reason::March)
                    };
                    self.general_override_stance(g, id, stance, reason);
                }
            }
            changed.extend(ids);
        }
        if complete && let Some(army) = self.economy.generals.armies.get_mut(&g) {
            army.routed = Some(to);
        }
    }
}
