use super::*;
impl GameWorld {
    pub(super) fn general_reinforce(
        &mut self,
        id: EntityId,
        player: PlayerId,
        work: &mut GeneralWork,
    ) {
        let mut counts = [0; 5];
        let Some(a) = self.economy.generals.armies.get(&id) else {
            return;
        };
        // Nothing is bought until the player presses Train.
        if !a.training {
            return;
        }
        let weights = a.mix;
        let mut cursors = a.production_scan;
        for &member in &a.members {
            if let Some(r) = self
                .unit(member)
                .filter(|u| u.hp > 0)
                .and_then(|u| role(u.kind))
            {
                counts[r] += 1;
            }
        }
        for &k in a.pending.values() {
            if let Some(r) = role(k) {
                counts[r] += 1;
            }
        }
        if ![
            UnitKind::Militia,
            UnitKind::Archer,
            UnitKind::Knight,
            UnitKind::BatteringRam,
            UnitKind::Scout,
        ]
        .into_iter()
        .enumerate()
        .any(|(r, kind)| weights[r] > 0 && a.room_for(kind))
        {
            // Full: stay idle until the player presses Train again.
            if let Some(a) = self.economy.generals.armies.get_mut(&id) {
                a.training = false;
            }
            return;
        }
        // The sliders' ratios; a role at zero is never bought. Cross
        // multiplication avoids floats.
        let mut roles: Vec<usize> = (0..5).filter(|&r| weights[r] > 0).collect();
        roles.sort_by(|&a, &b| {
            ((counts[a] + 1) * usize::from(weights[b]))
                .cmp(&((counts[b] + 1) * usize::from(weights[a])))
                .then(a.cmp(&b))
        });
        let Some(centre) = self.unit(id).map(|u| u.position) else {
            return;
        };
        let mut buildings = Vec::new();
        // Rotate bounded slices of each index. Out-of-sphere buildings must
        // not keep a later, eligible building hidden forever.
        for (slot, kind) in [
            crate::BuildingKind::Barracks,
            crate::BuildingKind::ArcheryRange,
            crate::BuildingKind::Stable,
            crate::BuildingKind::SiegeWorkshop,
            crate::BuildingKind::Castle,
        ]
        .into_iter()
        .enumerate()
        {
            let Some(ids) = self
                .economy
                .dropoffs
                .get(&(player, kind))
                .filter(|ids| !ids.is_empty())
            else {
                cursors[slot] = 0;
                continue;
            };
            let start = cursors[slot] % ids.len();
            let count = ids.len().min(25);
            for &building in ids[start..].iter().chain(&ids[..start]).take(count) {
                work.building_visits += 1;
                if let Some(b) = self.building(building)
                    && b.is_complete()
                    && b.hp > 0
                    && b.training_queue.is_empty()
                    && b.research.is_none()
                    && super::super::placement::position(b.footprint.min)
                        .is_ok_and(|p| in_influence(centre, p))
                {
                    buildings.push((b.id, b.kind));
                }
            }
            cursors[slot] = (start + count) % ids.len();
        }
        if let Some(a) = self.economy.generals.armies.get_mut(&id) {
            a.production_scan = cursors;
        }
        for r in roles {
            let kinds: &[UnitKind] = match r {
                0 => &[UnitKind::Militia, UnitKind::Spearman],
                1 => &[UnitKind::Archer, UnitKind::Skirmisher],
                2 => &[UnitKind::Knight],
                3 => &[UnitKind::BatteringRam, UnitKind::Mangonel],
                _ => &[UnitKind::Scout],
            };
            for &(building, kind) in &buildings {
                if self.economy.generals.recruits.contains_key(&building) {
                    continue;
                }
                for &unit in kinds {
                    if crate::trainable_units(kind).contains(&unit)
                        && self.economy.generals.armies[&id].room_for(unit)
                        && self.issue_train(player, building, unit).is_ok()
                    {
                        let Some(actual) = self
                            .building(building)
                            .and_then(|b| b.training_queue.back().copied())
                            .map(|e| e.kind)
                        else {
                            return;
                        };
                        let Some(a) = self.economy.generals.armies.get_mut(&id) else {
                            return;
                        };
                        a.pending.insert(building, actual);
                        self.economy
                            .generals
                            .recruits
                            .insert(building, (id, actual));
                        work.orders += 1;
                        return;
                    }
                }
            }
        }
    }
    pub(crate) fn cancel_general_recruit(
        &mut self,
        building: EntityId,
    ) -> Option<(EntityId, UnitKind)> {
        let recruit = self.economy.generals.recruits.remove(&building)?;
        if let Some(a) = self.economy.generals.armies.get_mut(&recruit.0) {
            a.pending.remove(&building);
        }
        Some(recruit)
    }
    pub(crate) fn finish_general_recruit(&mut self, building: EntityId, unit: EntityId) {
        let Some((g, _)) = self.cancel_general_recruit(building) else {
            return;
        };
        let Some(u) = self.unit(unit) else { return };
        let owned = self.unit(g).map_or_else(
            || {
                // A sheltered commander has no map record. Only completion
                // needs this indexed Castle lookup; ordinary ticks do not.
                self.player_buildings_of(u.player, crate::BuildingKind::Castle)
                    .any(|b| {
                        b.garrison.iter().any(|held| {
                            held.unit.id == g
                                && held.unit.kind == UnitKind::General
                                && held.unit.player == u.player
                                && held.unit.hp > 0
                        })
                    })
            },
            |leader| leader.player == u.player && leader.kind == UnitKind::General && leader.hp > 0,
        );
        if owned {
            self.claim_general_unit(g, u);
        }
    }
}
