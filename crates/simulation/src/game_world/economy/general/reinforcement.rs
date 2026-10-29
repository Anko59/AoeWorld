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
        for &member in &a.members {
            if let Some(r) = self.unit(member).and_then(|u| role(u.kind)) {
                counts[r] += 1;
            }
        }
        for &k in a.pending.values() {
            if let Some(r) = role(k) {
                counts[r] += 1;
            }
        }
        if counts.iter().sum::<usize>() >= GENERAL_ARMY_LIMIT {
            return;
        }
        // Ratios 35:30:20:10:5. Cross multiplication avoids floats.
        let weights = [35, 30, 20, 10, 5];
        let mut roles = [0, 1, 2, 3, 4];
        roles.sort_by(|&a, &b| {
            ((counts[a] + 1) * weights[b])
                .cmp(&((counts[b] + 1) * weights[a]))
                .then(a.cmp(&b))
        });
        let mut visited = 0;
        // Sample each production index separately: economic buildings and a
        // large Barracks count must not hide Ranges, Stables or Workshops.
        let buildings: Vec<_> = [
            crate::BuildingKind::Barracks,
            crate::BuildingKind::ArcheryRange,
            crate::BuildingKind::Stable,
            crate::BuildingKind::SiegeWorkshop,
            crate::BuildingKind::Castle,
        ]
        .into_iter()
        .flat_map(|kind| self.player_buildings_of(player, kind).take(25))
        .inspect(|_| visited += 1)
        .filter(|b| {
            b.is_complete() && b.hp > 0 && b.training_queue.is_empty() && b.research.is_none()
        })
        .map(|b| (b.id, b.kind))
        .collect();
        work.building_visits += visited;
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
                        && self.issue_train(player, building, unit).is_ok()
                    {
                        let Some(actual) = self
                            .building(building)
                            .and_then(|b| b.training_queue.back())
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
        if let Some((g, _)) = self.cancel_general_recruit(building)
            && let (Some(u), Some(leader)) = (self.unit(unit), self.unit(g))
            && u.player == leader.player
            && leader.hp > 0
        {
            self.claim_general_unit(g, u);
        }
    }
}
