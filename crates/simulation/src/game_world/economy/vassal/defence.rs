use super::*;
use crate::GameUnit;
use aoe_map::{LandBucket, RegionReachability};

/// Nearby field commanders are close enough to act as a local reserve.
const CALL_RADIUS: i32 = 32;
/// Tiles a threat must move from where a General was sent to redirect him.
const THREAT_MOVED_TILES: i64 = 8;

impl GameWorld {
    pub(super) fn vassal_squads(&self, id: EntityId) -> [VassalSquad; 5] {
        let mut squads = [VassalSquad::default(); 5];
        for unit in self.economy.vassals.domains[&id]
            .soldiers
            .iter()
            .filter_map(|&soldier| self.unit(soldier))
        {
            let Some(role) = super::super::general::role(unit.kind) else {
                continue;
            };
            let squad = &mut squads[role];
            if unit.task == UnitTask::Attacking || unit.attack_target.is_some() {
                squad.attacking += 1;
            } else if unit.task == UnitTask::Moving || unit.moving || unit.planning {
                squad.moving += 1;
            } else {
                squad.holding += 1;
            }
        }
        squads
    }

    pub(super) fn vassal_defend(
        &mut self,
        id: EntityId,
        units: &[GameUnit],
        changed: &mut Vec<EntityId>,
    ) {
        let Some(d) = self.economy.vassals.domains.get(&id) else {
            return;
        };
        let (player, center) = (d.player, d.center);
        let threat = units
            .iter()
            .filter(|u| {
                u.player != player
                    && u.hp > 0
                    && self.vassal_owns(id, u.position)
                    && self.unit_visible_to(player, u.id)
            })
            .min_by_key(|u| u.id)
            .map(|u| (u.id, u.position))
            .or_else(|| self.vassal_recent_attacker(id, units));
        let soldiers: Vec<_> = d.soldiers.iter().copied().collect();
        if let Some((_, at)) = threat {
            // The raiders' side is walled first: re-plan when it changes.
            let tile = at.tile_floor();
            if let Some(d) = self.economy.vassals.domains.get_mut(&id)
                && d.hot.is_none_or(|h| {
                    super::ring::side_of(d.center, h) != super::ring::side_of(d.center, tile)
                })
            {
                d.request_replan(self.tick.0);
                d.hot = Some(tile);
            }
        }
        let wolves = if soldiers.is_empty() {
            Vec::new()
        } else {
            let victims = units
                .iter()
                .filter(|u| u.player == player && u.hp > 0 && self.vassal_owns(id, u.position))
                .map(|u| u.id)
                .collect();
            self.wolves_hunting(&victims)
        };
        for soldier in soldiers {
            if let Some((wolf, _)) = wolves.first().filter(|_| threat.is_none()) {
                self.record_vassal_decision();
                let _ = self.issue_attack(player, soldier, *wolf);
                changed.push(soldier);
                continue;
            }
            if let Some(enemy) = threat {
                self.record_vassal_decision();
                let _ = self.issue_attack(player, soldier, enemy.0);
            } else if self
                .unit(soldier)
                .is_some_and(|u| u.task == UnitTask::Attacking || !self.vassal_owns(id, u.position))
                && let Ok(to) =
                    WorldPosition::from_tile_center(TileCoord::new(center.x, center.y + 4))
                && (self
                    .unit(soldier)
                    .is_some_and(|u| u.attack_target.is_some())
                    || !self.moving_towards(soldier, to))
            {
                self.record_vassal_decision();
                let _ = self.issue_move(soldier, to);
            }
            changed.push(soldier);
        }
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.status.alert = threat.is_some();
        }
        if let Some(enemy) = threat {
            let generals: Vec<_> = units
                .iter()
                .filter(|u| {
                    u.kind == UnitKind::General
                        && u.player == player
                        && u.hp > 0
                        && within(center, u.position, CALL_RADIUS)
                        && !self.embarked(u.id)
                        && !self.economy.vassals.recalls.contains_key(&u.id)
                        && self.vassal_help_reachable(u, enemy.1)
                })
                .map(|u| u.id)
                .collect();
            for g in generals {
                let Some(a) = self.economy.generals.armies.get(&g) else {
                    continue;
                };
                let Some(u) = self.unit(g) else {
                    continue;
                };
                let saved = Recall {
                    vassal: id,
                    destination: a.destination,
                    position: u.position,
                    movement: self.movement_order(g).map(|o| o.destination),
                    solo: a.solo,
                    called: enemy.1,
                };
                self.economy.vassals.returns.remove(&g);
                self.record_vassal_decision();
                self.general_march(g, enemy.1);
                self.economy.vassals.recalls.insert(g, saved);
            }
            self.follow_vassal_threat(id, enemy.1);
        } else {
            self.finish_vassal_recall(id);
        }
        let responding = self
            .economy
            .vassals
            .recalls
            .values()
            .filter(|r| r.vassal == id)
            .count() as u16;
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.status.responding = responding;
        }
    }
    fn vassal_help_reachable(&self, general: &GameUnit, destination: WorldPosition) -> bool {
        LandBucket::for_radius(general.kind.collision_radius()).is_some_and(|bucket| {
            self.land_reachable_bounded(
                general.player,
                bucket,
                general.position.tile_floor(),
                destination.tile_floor(),
            )
            .is_ok_and(RegionReachability::is_reachable)
        })
    }

    /// A recalled General marches to where the threat was seen. Raiders that
    /// walked on from there left him standing out of sight of them forever,
    /// so the recall never ended: send him on when the threat has moved.
    fn follow_vassal_threat(&mut self, id: EntityId, threat: WorldPosition) {
        let moved: Vec<_> = self
            .economy
            .vassals
            .recalls
            .iter()
            .filter(|(g, recall)| {
                recall.vassal == id
                    && super::super::general::distance(recall.called, threat)
                        > (THREAT_MOVED_TILES * 1024).pow(2)
                    && self
                        .economy
                        .generals
                        .armies
                        .get(g)
                        .is_some_and(|a| a.destination == Some(recall.called) && !a.solo)
            })
            .map(|(&g, _)| g)
            .collect();
        for g in moved {
            if self
                .unit(g)
                .is_none_or(|u| !self.vassal_help_reachable(&u, threat))
            {
                continue;
            }
            self.record_vassal_decision();
            self.general_march(g, threat);
            if let Some(recall) = self.economy.vassals.recalls.get_mut(&g) {
                recall.called = threat;
            }
        }
    }
    pub(super) fn finish_vassal_recall(&mut self, id: EntityId) {
        let generals: Vec<_> = self
            .economy
            .vassals
            .recalls
            .iter()
            .filter(|(_, r)| r.vassal == id)
            .map(|(&g, _)| g)
            .collect();
        for g in generals {
            let Some(recall) = self.economy.vassals.recalls.remove(&g) else {
                continue;
            };
            // A later owner march/solo command wins over the automatic return.
            if self
                .economy
                .generals
                .armies
                .get(&g)
                .is_some_and(|a| a.destination == Some(recall.called) && !a.solo)
            {
                if recall.solo {
                    if let Some(u) = self.unit(g) {
                        self.record_vassal_decision();
                        let _ = self.issue_general_move(
                            u.player,
                            g,
                            recall.movement.unwrap_or(recall.position),
                        );
                    }
                } else if let Some(destination) = recall.destination {
                    self.record_vassal_decision();
                    self.general_march(g, destination);
                } else {
                    self.general_march(g, recall.position);
                    self.economy.vassals.returns.insert(g, recall.position);
                }
            }
        }
    }
    pub(super) fn finish_vassal_returns(&mut self) {
        let returns: Vec<_> = self
            .economy
            .vassals
            .returns
            .iter()
            .map(|(&g, &goal)| (g, goal))
            .collect();
        for (g, goal) in returns {
            if !self
                .economy
                .generals
                .armies
                .get(&g)
                .is_some_and(|a| a.destination == Some(goal) && !a.solo)
            {
                self.economy.vassals.returns.remove(&g);
            } else if let Some(u) = self
                .unit(g)
                .filter(|u| u.position.tile_floor() == goal.tile_floor())
            {
                self.record_vassal_decision();
                let _ = self.issue_stop(u.player, g);
                self.economy.vassals.returns.remove(&g);
            }
        }
    }
}
