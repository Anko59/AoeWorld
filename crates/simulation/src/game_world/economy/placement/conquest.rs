use super::*;
use crate::game_world::diplomacy::reset_conquered_unit;

impl GameWorld {
    /// Keep the existing footprint, construction progress and entity identity.
    /// Reindex the owner and cancel work commissioned by the defeated seat.
    pub(crate) fn transfer_conquered_buildings(&mut self, from: PlayerId, to: PlayerId) {
        let ids: Vec<_> = self
            .player_buildings(from)
            .map(|building| building.id)
            .collect();
        for id in ids {
            let Some(building) = self.building(id) else {
                continue;
            };
            let (kind, footprint, span) = (building.kind, building.footprint, building.bridge_span);
            let (complete, planned) = (building.is_complete(), building.is_unstarted());
            // Two seats can reserve the same unexplored site independently.
            // Keep the winner's existing reservation and recover the cost of
            // the duplicate plan into the estate that is about to transfer.
            if planned
                && self.site_has_building(footprint, span, Some(to))
                && self.issue_cancel_construction(from, id).is_ok()
            {
                continue;
            }
            self.wake_construction(id);
            self.forget_queues(id);
            self.cancel_general_recruit(id);
            self.economy.active_training.remove(&id);
            self.economy.active_research.remove(&id);
            self.economy.town_bells.remove(&id);
            self.economy.dispatch.sites.remove(&id);
            self.combat.tower_targets.remove(&id);
            self.combat.target_rounds.remove(&id);
            self.update_observer(id, None);
            self.forget_building(id, from, footprint);
            if planned {
                self.remove_foundation_reservation(id, from, footprint, span);
            }
            if !complete {
                self.unindex_foundation(id, from, footprint);
            }
            if let Some(bucket) = self.economy.dropoffs.get_mut(&(from, kind))
                && let Ok(at) = bucket.binary_search(&id)
            {
                bucket.remove(at);
            }
            let bucket = self.economy.dropoffs.entry((to, kind)).or_default();
            if let Err(index) = bucket.binary_search(&id) {
                bucket.insert(index, id);
            }

            let Some(building) = self.building_mut(id) else {
                continue;
            };
            building.player = to;
            building.last_hit = None;
            building.training_queue.clear();
            building.research = None;
            building.rally_point = None;
            let occupants = building
                .garrison
                .iter_mut()
                .map(|occupant| {
                    let old = occupant.unit.player;
                    let kind = occupant.unit.kind;
                    if old == from {
                        reset_conquered_unit(&mut occupant.unit, to);
                        occupant.intent = Default::default();
                        occupant.steward = None;
                        occupant.vassal = None;
                    }
                    (old, kind)
                })
                .collect::<Vec<_>>();
            for (old, kind) in occupants {
                if old == from {
                    self.change_unit_census(from, Some(kind), None, 1);
                    self.change_unit_census(to, None, Some(kind), 1);
                }
            }
            if !complete {
                self.index_foundation(id, to, footprint);
            }
            self.index_new_building(id, to, footprint);
            self.steward_building_added(id, to, kind, footprint);
            self.update_building_vision(id);
            self.mark_building_changed(id);
        }
    }
}
