use super::*;

const BELL_RANGE: i32 = 25;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Work {
    Gather(u64),
    Farm(EntityId),
    Hunt(EntityId),
    Build(EntityId),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ShelterIntent {
    pub(crate) work: Option<Work>,
    pub(crate) bell: Option<EntityId>,
}

fn center(rect: TileRect) -> aoe_core::TileCoord {
    aoe_core::TileCoord::new(
        rect.min.x + rect.width() / 2,
        rect.min.y + rect.height() / 2,
    )
}

impl GameWorld {
    pub(super) fn shelter_intent(&self, kind: UnitKind, unit: EntityId) -> ShelterIntent {
        if kind != UnitKind::Villager {
            return ShelterIntent::default();
        }
        let work = self
            .hunting_target(unit)
            .map(Work::Hunt)
            .or_else(|| {
                self.economy.gatherers.get(&unit).map(|task| {
                    if let Some(animal) = task.animal {
                        Work::Hunt(animal)
                    } else if let Some(farm) = task.farm {
                        Work::Farm(farm)
                    } else {
                        Work::Gather(task.node)
                    }
                })
            })
            .or_else(|| {
                self.economy
                    .builders
                    .get(&unit)
                    .map(|task| Work::Build(task.building))
            });
        ShelterIntent { work, bell: None }
    }

    pub(super) fn resume_sheltered_work(
        &mut self,
        player: PlayerId,
        unit: EntityId,
        intent: ShelterIntent,
    ) {
        match intent.work {
            Some(Work::Gather(node)) => {
                let _ = self.issue_gather(player, unit, node);
            }
            Some(Work::Farm(farm)) => {
                let _ = self.issue_farm(player, unit, farm);
            }
            Some(Work::Hunt(animal)) => {
                let _ = self.issue_hunt(player, &[unit], animal);
            }
            Some(Work::Build(building)) => {
                let _ = self.issue_construct(player, &[unit], building);
            }
            None => {}
        }
    }

    pub fn issue_back_to_work(
        &mut self,
        player: PlayerId,
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let shelter = self.building(building).ok_or(EconomyError::UnknownEntity)?;
        if shelter.player != player {
            return Err(EconomyError::NotOwner);
        }
        if shelter.kind.garrison_capacity() == 0 {
            return Err(EconomyError::WrongUnitKind);
        }
        self.release_matching(
            building,
            |occupant| occupant.unit.kind == UnitKind::Villager,
            true,
        );
        Ok(())
    }

    pub fn issue_town_bell(
        &mut self,
        player: PlayerId,
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let shelter = self.building(building).ok_or(EconomyError::UnknownEntity)?;
        if shelter.player != player {
            return Err(EconomyError::NotOwner);
        }
        if shelter.kind != BuildingKind::TownCenter {
            return Err(EconomyError::WrongUnitKind);
        }
        if !shelter.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        let origin = center(shelter.footprint);
        let range = u64::from(BELL_RANGE.unsigned_abs()).pow(2);
        let shelters: Vec<_> = [
            BuildingKind::TownCenter,
            BuildingKind::Castle,
            BuildingKind::WatchTower,
        ]
        .into_iter()
        .flat_map(|kind| self.player_buildings_of(player, kind))
        .filter(|b| {
            b.is_complete() && b.hp > 0 && distance_squared(origin, center(b.footprint)) <= range
        })
        .map(|b| (b.id, center(b.footprint)))
        .collect();
        if self.economy.town_bells.remove(&building) {
            let pending: Vec<_> = self
                .economy
                .shelter_intents
                .iter()
                .filter_map(|(&unit, intent)| {
                    (intent.bell == Some(building)).then_some((unit, *intent))
                })
                .collect();
            for (unit, intent) in pending {
                if self.issue_stop(player, unit).is_ok() {
                    self.resume_sheltered_work(player, unit, intent);
                }
            }
            for (id, _) in shelters {
                self.release_matching(id, |occupant| occupant.intent.bell == Some(building), true);
                if self.economy.buildings.get(&id).is_some_and(|shelter| {
                    shelter
                        .garrison
                        .iter()
                        .any(|occupant| occupant.intent.bell == Some(building))
                }) {
                    self.economy.town_bells.insert(building);
                }
            }
            return Ok(());
        }

        self.economy.town_bells.insert(building);
        let rect = TileRect::from_xywh(
            origin.x - BELL_RANGE,
            origin.y - BELL_RANGE,
            2 * BELL_RANGE + 1,
            2 * BELL_RANGE + 1,
        );
        let mut villagers = self.query(rect).0;
        villagers.retain(|u| {
            u.player == player
                && u.kind == UnitKind::Villager
                && u.hp > 0
                && !self.economy.garrison_orders.contains_key(&u.id)
                && distance_squared(origin, u.position.tile_floor()) <= range
        });
        villagers.sort_unstable_by_key(|u| u.id);
        let mut candidates = shelters;
        for villager in villagers {
            let tile = villager.position.tile_floor();
            candidates.sort_unstable_by_key(|&(id, at)| (distance_squared(tile, at), id));
            for &(id, _) in &candidates {
                if self.issue_garrison(player, villager.id, id).is_ok() {
                    self.economy
                        .shelter_intents
                        .entry(villager.id)
                        .or_default()
                        .bell = Some(building);
                    break;
                }
            }
        }
        Ok(())
    }
}
