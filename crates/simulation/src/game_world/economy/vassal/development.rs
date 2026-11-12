use super::*;
use crate::Stockpile;

/// Foundations one pass may staff, and new placements it may attempt.
const STAFFED_PER_PASS: usize = 16;
const ATTEMPTS_PER_PASS: usize = 8;
/// Foundations kept laid ahead per builder.
const PIPELINE: usize = 12;
/// Wall tiles one placement lays as a single line.
const WALL_RUN: usize = 10;

/// Builders per order. Several builders share one foundation at diminishing
/// returns, so a wall or road run gets one builder (each run then goes up
/// beside the others); towers, gates and production take a crew.
fn crew_size(kind: BuildingKind) -> usize {
    if kind.is_wall() || kind == BuildingKind::Road {
        1
    } else {
        3
    }
}

impl GameWorld {
    /// A site is done when its building stands. Stone satisfies a palisade
    /// site, never the other way round: a palisade on a stone site is
    /// replaced (see `upgradable`).
    fn vassal_site_done(&self, player: PlayerId, site: plan::Site) -> bool {
        self.building_at(site.tile)
            .and_then(|b| self.building(b))
            .is_some_and(|b| {
                b.player == player
                    && b.is_complete()
                    && (b.kind == site.kind
                        || b.kind == BuildingKind::StoneWall
                            && site.kind == BuildingKind::PalisadeWall
                        || b.kind == BuildingKind::StoneGate
                            && site.kind == BuildingKind::PalisadeGate)
            })
    }

    /// A finished own palisade wall or gate standing where stone is planned.
    fn upgradable(
        &self,
        player: PlayerId,
        tile: TileCoord,
        kind: BuildingKind,
    ) -> Option<EntityId> {
        let id = self.building_at(tile)?;
        let b = self.building(id)?;
        (b.player == player
            && b.is_complete()
            && (kind == BuildingKind::StoneWall && b.kind == BuildingKind::PalisadeWall
                || kind == BuildingKind::StoneGate && b.kind == BuildingKind::PalisadeGate))
            .then_some(id)
    }

    /// The worker nearest `to`, busy or not.
    fn nearest_worker(&self, workers: &[EntityId], to: TileCoord) -> Vec<EntityId> {
        workers
            .iter()
            .copied()
            .min_by_key(|w| {
                (
                    self.unit(*w).map_or(u64::MAX, |u| {
                        super::super::placement::distance_squared(u.position.tile_floor(), to)
                    }),
                    *w,
                )
            })
            .into_iter()
            .collect()
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

    /// Idle domain soldiers crowding a foundation's edge kept its builders
    /// from reaching it (a tower by the castle stalled at zero work behind
    /// twenty militia): they walk to the muster point beside the castle.
    fn clear_vassal_soldiers(&mut self, id: EntityId, footprint: TileRect) {
        let Some(d) = self.economy.vassals.domains.get(&id) else {
            return;
        };
        let muster = TileCoord::new(d.center.x, d.center.y + 4);
        let near = TileRect::new(
            TileCoord::new(footprint.min.x - 2, footprint.min.y - 2),
            TileCoord::new(footprint.max.x + 2, footprint.max.y + 2),
        );
        let crowd: Vec<_> = d
            .soldiers
            .iter()
            .copied()
            .filter(|s| {
                self.unit(*s).is_some_and(|u| {
                    u.task == UnitTask::Idle && !u.moving && near.contains(u.position.tile_floor())
                })
            })
            .collect();
        if near.contains(muster) {
            return;
        }
        for s in crowd {
            if let Ok(to) = WorldPosition::from_tile_center(muster) {
                self.record_vassal_decision();
                let _ = self.issue_move(s, to);
            }
        }
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
        let developed = deficits.is_empty() && d.pending.is_none();
        let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        let status = &mut d.status;
        status.completed = u16::try_from(sites.len() - deficits.len()).unwrap_or(u16::MAX);
        status.total = u16::try_from(sites.len()).unwrap_or(u16::MAX);
        status.developed = developed;
        status.next = deficits.first().map(|(_, site)| site.kind);
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
        for (_, site) in &deficits {
            let tile = &site.tile;
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
            let (building, done, target, footprint, kind) = (
                building.id,
                building.work_done,
                building.footprint.min,
                building.footprint,
                building.kind,
            );
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
                    domain.request_replan(tick + plan::REFUSED_REPLAN_TICKS);
                    domain.progress = None;
                    for worker in &workers {
                        if self
                            .economy
                            .builders
                            .get(worker)
                            .is_some_and(|job| job.building == building)
                        {
                            self.record_vassal_decision();
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
            self.clear_vassal_soldiers(id, footprint);
            let crew = self.nearest_idle(&mut idle, target, crew_size(kind));
            if !crew.is_empty() {
                self.record_vassal_decision();
                let _ = self.issue_construct(player, &crew, building);
            }
        }
        // Foundations are laid ahead of the builders (they go on to the next
        // nearby one by themselves): every builder always has a run waiting.
        let mut open = deficits
            .iter()
            .filter(|(_, site)| {
                self.building_at(site.tile)
                    .and_then(|b| self.building(b))
                    .is_some_and(|b| b.player == player && !b.is_complete())
            })
            .count();
        let want = workers.len() * PIPELINE;
        if idle.is_empty() && open >= want {
            return;
        }
        // Sites go by tier (home towers, the threatened wall, roads and
        // production, ...); a refused site does not block the others.
        let mut wait = VassalWait::Blocked;
        let mut candidates: Vec<_> = deficits
            .iter()
            .copied()
            .filter(|(_, site)| {
                (self.building_at(site.tile).is_none()
                    || self.upgradable(player, site.tile, site.kind).is_some())
                    && !self.economy.vassals.domains[&id]
                        .refused
                        .contains_key(&site.tile)
            })
            .collect();
        // Within a tier, the site nearest an idle builder: crews lay
        // neighbouring runs instead of crossing the domain for each one.
        let anchor = idle
            .iter()
            .chain(&workers)
            .min()
            .and_then(|w| self.unit(*w))
            .map_or(TileCoord::new(0, 0), |u| u.position.tile_floor());
        candidates.sort_by_key(|(i, site)| {
            (
                site.tier,
                super::super::placement::distance_squared(anchor, site.tile),
                *i < cursor,
                *i,
            )
        });
        let mut placed = BTreeSet::new();
        for (i, site) in candidates.into_iter().take(ATTEMPTS_PER_PASS * 4) {
            let plan::Site {
                kind,
                tile,
                vertical,
                ..
            } = site;
            if (idle.is_empty() && open >= want) || placed.len() >= ATTEMPTS_PER_PASS {
                break;
            }
            if placed.contains(&tile) {
                continue;
            }
            // A wall extends along its side over the following deficits.
            let mut run = vec![tile];
            if kind.is_wall() || kind == BuildingKind::Road {
                for next in sites.iter().skip(i + 1) {
                    let (k, t) = (&next.kind, &next.tile);
                    let last = run[run.len() - 1];
                    if run.len() == WALL_RUN
                        || *k != kind
                        || (t.x - last.x).abs() + (t.y - last.y).abs() != 1
                        || (run.len() > 1
                            && (t.x - last.x, t.y - last.y)
                                != (last.x - run[run.len() - 2].x, last.y - run[run.len() - 2].y))
                        || (self.building_at(*t).is_some()
                            && self.upgradable(player, *t, kind).is_none())
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
            // Stone over palisade: the palisade run comes down as the stone
            // run goes up, in one step. If the stone run is refused, the
            // palisade goes back up so the line never opens.
            let mut replaced = Vec::new();
            for t in &run {
                if let Some(old) = self.upgradable(player, *t, kind)
                    && let Some(b) = self.building(old)
                {
                    replaced.push((b.kind, b.footprint));
                    self.remove_building(old);
                }
            }
            // With every builder busy, the nearest one queues the new run.
            let queued = idle.is_empty();
            let crew = if queued {
                self.nearest_worker(&workers, tile)
            } else {
                self.nearest_idle(&mut idle, tile, crew_size(kind))
            };
            self.record_vassal_decision();
            let result = if kind.is_wall() || kind == BuildingKind::Road {
                self.issue_build_wall(player, &crew, kind, tile, run[run.len() - 1], queued)
                    .map(|_| ())
            } else if let Some(end) = site.end {
                self.issue_build_bridge(player, &crew, tile, end, queued)
                    .map(|_| ())
            } else {
                self.issue_build_gate(player, &crew, kind, tile, vertical, queued)
                    .map(|_| ())
            };
            if result.is_err() {
                for &(old, footprint) in &replaced {
                    let _ = self.insert_building(player, old, footprint, true);
                }
            }
            match result {
                Ok(()) => {
                    wait = VassalWait::Building;
                    open += run.len();
                    placed.extend(run);
                }
                // A full queue only means the builder has enough work waiting.
                Err(_) if queued => break,
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
                        d.request_replan(self.tick.0 + plan::REFUSED_REPLAN_TICKS);
                    }
                }
            }
        }
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.status.wait = wait;
        }
    }
}
