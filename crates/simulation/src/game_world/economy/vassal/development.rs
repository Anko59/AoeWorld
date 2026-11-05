use super::*;
use crate::Stockpile;

/// Foundations one pass may staff, and new placements it may attempt.
const STAFFED_PER_PASS: usize = 16;
const ATTEMPTS_PER_PASS: usize = 8;
/// Wall tiles one placement lays as a single line.
const WALL_RUN: usize = 10;

fn stage(kind: BuildingKind) -> u8 {
    match kind {
        BuildingKind::WatchTower => 0,
        BuildingKind::Barracks | BuildingKind::ArcheryRange | BuildingKind::Stable => 1,
        BuildingKind::Road => 3,
        _ => 2,
    }
}

impl GameWorld {
    fn vassal_site_done(&self, player: PlayerId, (kind, tile, _): plan::Site) -> bool {
        self.building_at(tile)
            .and_then(|b| self.building(b))
            .is_some_and(|b| b.player == player && b.kind == kind && b.is_complete())
    }

    fn nearest_idle(&self, idle: &mut Vec<EntityId>, to: TileCoord, n: usize) -> Vec<EntityId> {
        idle.sort_by_key(|w| {
            (
                self.unit(*w).map_or(u64::MAX, |u| {
                    super::super::placement::distance_squared(u.position.tile_floor(), to)
                }),
                *w,
            )
        });
        idle.drain(..idle.len().min(n)).collect()
    }

    pub(super) fn vassal_develop(&mut self, id: EntityId) {
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.refused.retain(|_, until| *until > self.tick.0);
        } else {
            return;
        }
        let sites = self.vassal_plan(id);
        let d = &self.economy.vassals.domains[&id];
        let (player, enabled, cursor) = (d.player, d.development, d.cursor);
        let deficits: Vec<_> = sites
            .iter()
            .enumerate()
            .filter(|(_, site)| !self.vassal_site_done(player, **site))
            .map(|(i, site)| (i, *site))
            .collect();
        let developed = deficits.is_empty();
        let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        let status = &mut d.status;
        status.completed = u16::try_from(sites.len() - deficits.len()).unwrap_or(u16::MAX);
        status.total = u16::try_from(sites.len()).unwrap_or(u16::MAX);
        status.developed = developed;
        status.next = deficits.first().map(|(_, (k, _, _))| *k);
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
        let workers: Vec<_> = self.economy.vassals.domains[&id]
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
        let mut idle: Vec<_> = workers
            .iter()
            .copied()
            .filter(|w| !self.economy.builders.contains_key(w))
            .collect();
        // Staff paid foundations first, several at once, before buying more.
        let mut first = true;
        let mut staffed = 0;
        for (_, (_, tile, _)) in &deficits {
            if staffed == STAFFED_PER_PASS {
                break;
            }
            if self.economy.vassals.domains[&id].refused.contains_key(tile) {
                continue;
            }
            let Some(building) = self
                .building_at(*tile)
                .and_then(|b| self.building(b))
                .filter(|b| b.player == player && !b.is_complete())
            else {
                continue;
            };
            staffed += 1;
            let (building, done, target) =
                (building.id, building.work_done, building.footprint.min);
            if first {
                first = false;
                let tick = self.tick.0;
                let stall = u64::from(self.config.tick_hz) * 60;
                let Some(domain) = self.economy.vassals.domains.get_mut(&id) else {
                    return;
                };
                if domain.progress.is_some_and(|(site, work, since)| {
                    site == building && work == done && tick.saturating_sub(since) >= stall
                }) {
                    domain.refused.insert(*tile, tick + 600);
                    domain.planned = domain
                        .planned
                        .min(tick.saturating_sub(plan::REPLAN_TICKS - plan::REFUSED_REPLAN_TICKS));
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
                    domain.progress = Some((building, done, tick));
                }
            }
            let crew = self.nearest_idle(&mut idle, target, 3);
            if !crew.is_empty() {
                let _ = self.issue_construct(player, &crew, building);
            }
        }
        if idle.is_empty() || staffed > 0 && idle.len() < 3 {
            return;
        }
        // Towers and production precede walls; a refused site does not block
        // other sites in the same stage. Roads wait for the fortification.
        let mut wait = VassalWait::Blocked;
        let fortifying = deficits
            .iter()
            .any(|(_, (k, _, _))| *k != BuildingKind::Road);
        let mut candidates: Vec<_> = deficits
            .iter()
            .copied()
            .filter(|(_, (kind, tile, _))| {
                (!fortifying || *kind != BuildingKind::Road)
                    && self.building_at(*tile).is_none()
                    && !self.economy.vassals.domains[&id].refused.contains_key(tile)
            })
            .collect();
        // Within a stage, the site nearest an idle builder: crews lay
        // neighbouring runs instead of crossing the domain for each one.
        let anchor = idle
            .iter()
            .min()
            .and_then(|w| self.unit(*w))
            .map_or(TileCoord::new(0, 0), |u| u.position.tile_floor());
        candidates.sort_by_key(|(i, (kind, tile, _))| {
            (
                stage(*kind),
                super::super::placement::distance_squared(anchor, *tile),
                *i < cursor,
                *i,
            )
        });
        let mut placed = BTreeSet::new();
        for (i, (kind, tile, vertical)) in candidates.into_iter().take(ATTEMPTS_PER_PASS * 4) {
            if idle.is_empty() || placed.len() >= ATTEMPTS_PER_PASS {
                break;
            }
            if placed.contains(&tile) {
                continue;
            }
            // A wall extends along its side over the following deficits.
            let mut run = vec![tile];
            if kind.is_wall() {
                for (k, t, _) in sites.iter().skip(i + 1) {
                    let last = run[run.len() - 1];
                    if run.len() == WALL_RUN
                        || *k != kind
                        || (t.x - last.x).abs() + (t.y - last.y).abs() != 1
                        || (run.len() > 1
                            && (t.x - last.x, t.y - last.y)
                                != (last.x - run[run.len() - 2].x, last.y - run[run.len() - 2].y))
                        || self.building_at(*t).is_some()
                        || self.economy.vassals.domains[&id].refused.contains_key(t)
                    {
                        break;
                    }
                    run.push(*t);
                }
            }
            if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
                d.cursor = (i + run.len()) % sites.len();
            }
            let n = run.len() as u32;
            let cost = kind.rules().cost;
            let reserve = Stockpile {
                food: 200,
                wood: 200,
                gold: 100,
                stone: 200,
            };
            if self.player_economy(player).is_none_or(|p| {
                p.stockpile.food < reserve.food + cost.food * n
                    || p.stockpile.wood < reserve.wood + cost.wood * n
                    || p.stockpile.gold < reserve.gold + cost.gold * n
                    || p.stockpile.stone < reserve.stone + cost.stone * n
            }) {
                wait = VassalWait::Reserve;
                break;
            }
            let crew = self.nearest_idle(&mut idle, tile, 3);
            let result = if kind.is_wall() {
                self.issue_build_wall(player, &crew, kind, tile, run[run.len() - 1], false)
                    .map(|_| ())
            } else {
                self.issue_build_gate(player, &crew, kind, tile, vertical, false)
                    .map(|_| ())
            };
            match result {
                Ok(()) => {
                    wait = VassalWait::Building;
                    placed.extend(run);
                }
                Err(error) => {
                    idle.extend(crew);
                    if matches!(
                        error,
                        EconomyError::RequiresFeudalAge
                            | EconomyError::RequiresCastleAge
                            | EconomyError::RequiresImperialAge
                    ) {
                        wait = VassalWait::Age;
                    } else if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
                        d.refused.insert(tile, self.tick.0 + 600);
                        d.planned = d.planned.min(
                            self.tick
                                .0
                                .saturating_sub(plan::REPLAN_TICKS - plan::REFUSED_REPLAN_TICKS),
                        );
                    }
                }
            }
        }
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.status.wait = wait;
        }
    }
}
