use super::*;
use crate::{Age, Stockpile};

impl GameWorld {
    pub(super) fn vassal_develop(&mut self, id: EntityId) {
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.refused.retain(|_, until| *until > self.tick.0);
        } else {
            return;
        }
        let d = &self.economy.vassals.domains[&id];
        let (center, player, enabled, cursor) = (d.center, d.player, d.development, d.cursor);
        let age = self.player_economy(player).map_or(Age::Dark, |p| p.age);
        let sites = super::plan::sites(self, center, player, age, &d.refused);
        let deficits: Vec<_> = sites
            .iter()
            .enumerate()
            .filter(|(_, (kind, tile))| {
                !self
                    .building_at(*tile)
                    .and_then(|b| self.building(b))
                    .is_some_and(|b| b.player == player && b.kind == *kind && b.is_complete())
            })
            .map(|(i, site)| (i, *site))
            .collect();
        let developed = deficits.is_empty();
        let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        let status = &mut d.status;
        status.completed = (sites.len() - deficits.len()) as u16;
        status.total = sites.len() as u16;
        status.developed = developed;
        status.next = deficits.first().map(|(_, (k, _))| *k);
        status.wait = if developed {
            VassalWait::Developed
        } else if !enabled {
            VassalWait::Paused
        } else {
            VassalWait::Building
        };
        if developed || !enabled {
            self.release_vassal_builders(id);
            return;
        }
        let mut workers: Vec<_> = self.economy.vassals.domains[&id]
            .workers
            .iter()
            .copied()
            .filter(|&w| self.unit(w).is_some())
            .collect();
        if workers.is_empty() {
            if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
                d.status.wait = VassalWait::Builders;
            }
            return;
        }
        // Finish paid construction before purchasing another foundation.
        for (_, (_, tile)) in &deficits {
            if self.economy.vassals.domains[&id].refused.contains_key(tile) {
                continue;
            }
            if let Some(building) = self.building_at(*tile).and_then(|b| self.building(b))
                && building.player == player
                && !building.is_complete()
            {
                let target = building.footprint.min;
                let (building, done) = (building.id, building.work_done);
                let Some(domain) = self.economy.vassals.domains.get_mut(&id) else {
                    return;
                };
                if domain.progress.is_some_and(|(site, work, since)| {
                    site == building
                        && work == done
                        && self.tick.0.saturating_sub(since) >= u64::from(self.config.tick_hz) * 60
                }) {
                    domain.refused.insert(*tile, self.tick.0 + 600);
                    domain.progress = None;
                    for worker in &workers {
                        if self
                            .economy
                            .builders
                            .get(worker)
                            .is_some_and(|job| job.building == building)
                        {
                            let _ = self.issue_stop(player, *worker);
                        }
                    }
                    continue;
                }
                if domain
                    .progress
                    .is_none_or(|(site, work, _)| site != building || work != done)
                {
                    domain.progress = Some((building, done, self.tick.0));
                }
                workers.sort_by_key(|w| {
                    (
                        self.unit(*w).map_or(u64::MAX, |u| {
                            super::super::placement::distance_squared(
                                u.position.tile_floor(),
                                target,
                            )
                        }),
                        *w,
                    )
                });
                workers.truncate(3);
                let idle: Vec<_> = workers
                    .iter()
                    .copied()
                    .filter(|w| !self.economy.builders.contains_key(w))
                    .collect();
                if !idle.is_empty() {
                    let _ = self.issue_construct(player, &idle, building);
                }
                return;
            }
        }
        // Towers and production precede walls; a refused site does not block
        // other sites in the same stage. At most eight ordinary attempts/pass.
        let stage = |k| match k {
            BuildingKind::WatchTower => 0,
            BuildingKind::Barracks => 1,
            _ => 2,
        };
        let mut wait = VassalWait::Blocked;
        let mut candidates: Vec<_> = deficits;
        candidates.sort_by_key(|(i, (kind, _))| (stage(*kind), *i < cursor, *i));
        for (i, (kind, tile)) in candidates
            .into_iter()
            .filter(|(_, (_, tile))| !self.economy.vassals.domains[&id].refused.contains_key(tile))
            .take(8)
            .collect::<Vec<_>>()
        {
            if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
                d.cursor = (i + 1) % sites.len();
            }
            let cost = kind.rules().cost;
            let reserve = Stockpile {
                food: 200,
                wood: 200,
                gold: 100,
                stone: 200,
            };
            if self.player_economy(player).is_none_or(|p| {
                p.stockpile.food < reserve.food + cost.food
                    || p.stockpile.wood < reserve.wood + cost.wood
                    || p.stockpile.gold < reserve.gold + cost.gold
                    || p.stockpile.stone < reserve.stone + cost.stone
            }) {
                wait = VassalWait::Reserve;
                break;
            }
            workers.sort_by_key(|w| {
                (
                    self.unit(*w).map_or(u64::MAX, |u| {
                        super::super::placement::distance_squared(u.position.tile_floor(), tile)
                    }),
                    *w,
                )
            });
            let builders = &workers[..workers.len().min(3)];
            match self.issue_build(player, builders, kind, tile, false) {
                Ok(_) => {
                    wait = VassalWait::Building;
                    break;
                }
                Err(
                    EconomyError::RequiresFeudalAge
                    | EconomyError::RequiresCastleAge
                    | EconomyError::RequiresImperialAge,
                ) => {
                    wait = VassalWait::Age;
                }
                Err(_) => {
                    if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
                        d.refused.insert(tile, self.tick.0 + 600);
                    }
                }
            }
        }
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.status.wait = wait;
        }
    }
}
