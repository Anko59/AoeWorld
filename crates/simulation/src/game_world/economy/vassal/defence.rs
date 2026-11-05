use super::*;
use crate::GameUnit;
/// Tiles a threat must move from where a General was sent to redirect him.
const THREAT_MOVED_TILES: i64 = 8;

impl GameWorld {
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
                    && inside(center, u.position)
                    && self.unit_visible_to(player, u.id)
            })
            .min_by_key(|u| u.id)
            .map(|u| (u.id, u.position))
            .or_else(|| self.vassal_recent_attacker(id, units));
        let soldiers: Vec<_> = d.soldiers.iter().copied().collect();
        for soldier in soldiers {
            if let Some(enemy) = threat {
                let _ = self.issue_attack(player, soldier, enemy.0);
            } else if self
                .unit(soldier)
                .is_some_and(|u| u.task == UnitTask::Attacking || !inside(center, u.position))
                && let Ok(to) =
                    WorldPosition::from_tile_center(TileCoord::new(center.x, center.y + 4))
            {
                let _ = self.issue_move(soldier, to);
            }
            changed.push(soldier);
        }
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.status.alert = threat.is_some();
        }
        if let Some(enemy) = threat {
            let home = WorldPosition::new(center.x * 1024, center.y * 1024);
            let generals: Vec<_> = self
                .economy
                .generals
                .armies
                .keys()
                .copied()
                .filter(|g| {
                    !self.economy.vassals.recalls.contains_key(g)
                        && self.unit(*g).is_some_and(|u| {
                            u.player == player
                                && u.hp > 0
                                && super::super::general::distance(home, u.position)
                                    <= (100 * 1024_i64).pow(2)
                        })
                })
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
                        let _ = self.issue_general_move(
                            u.player,
                            g,
                            recall.movement.unwrap_or(recall.position),
                        );
                    }
                } else if let Some(destination) = recall.destination {
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
                let _ = self.issue_stop(u.player, g);
                self.economy.vassals.returns.remove(&g);
            }
        }
    }
}
