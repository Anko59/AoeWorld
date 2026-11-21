use super::*;
use crate::Stockpile;

/// New placements attempted per pass and total paid sites kept in flight.
const ATTEMPTS_PER_PASS: usize = 8;
const PIPELINE: usize = 24;
/// Wall tiles one placement lays as a single line.
const WALL_RUN: usize = 10;

const RESERVE: Stockpile = Stockpile {
    food: 200,
    wood: 200,
    gold: 100,
    stone: 200,
};

impl GameWorld {
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

    fn vassal_can_spend(&self, player: PlayerId, cost: Stockpile, n: u32) -> bool {
        self.player_economy(player).is_some_and(|p| {
            p.stockpile.food >= RESERVE.food + cost.food * n
                && p.stockpile.wood >= RESERVE.wood + cost.wood * n
                && p.stockpile.gold >= RESERVE.gold + cost.gold * n
                && p.stockpile.stone >= RESERVE.stone + cost.stone * n
        })
    }

    pub(super) fn vassal_develop(&mut self, id: EntityId) {
        let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        d.refused.retain(|_, until| *until > self.tick.0);
        let sites = self.vassal_plan(id);
        let d = &self.economy.vassals.domains[&id];
        let mut work = self.vassal_workload(d, &sites);
        let developed = work.deficits.is_empty() && work.open.is_empty() && d.pending.is_none();
        let enabled = d.development;
        let focus = d.status.focus;
        work.deficits
            .sort_by_key(|(_, site)| (site.build_priority(focus), site.tier));
        let Some(d) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        d.status.developed = developed;
        d.status.next = work.deficits.first().map(|(_, site)| site.kind);
        d.status.wait = if !enabled {
            VassalWait::Paused
        } else if developed {
            VassalWait::Developed
        } else if work.open.is_empty() {
            VassalWait::Blocked
        } else {
            VassalWait::Building
        };
        if enabled && !developed {
            for &(building, _) in &work.open {
                if let Some(footprint) = self.building(building).map(|b| b.footprint) {
                    self.clear_vassal_soldiers(id, footprint);
                }
            }
            self.observe_vassal_progress(id, &work);
            self.vassal_build(id, &sites, &work);
        }
        self.publish_vassal_work(id, work);
    }

    fn observe_vassal_progress(&mut self, id: EntityId, work: &super::work::Workload) {
        let previous = self.economy.vassals.domains[&id].progress;
        let Some((building, tile, done)) = work.open.iter().find_map(|(id, job)| {
            if !previous.is_some_and(|(site, _, _)| site == *id)
                && self.economy.builders.assigned_count(*id) == 0
            {
                return None;
            }
            self.building(*id).map(|b| (*id, job.tile, b.work_done))
        }) else {
            if let Some(domain) = self.economy.vassals.domains.get_mut(&id) {
                domain.progress = None;
            }
            return;
        };
        let tick = self.tick.0;
        let timeout = u64::from(self.config.tick_hz) * 60;
        let Some(domain) = self.economy.vassals.domains.get_mut(&id) else {
            return;
        };
        match domain.progress {
            Some((site, work, since)) if site == building && work == done => {
                if tick.saturating_sub(since) >= timeout && !domain.refused.contains_key(&tile) {
                    domain.refused.insert(tile, tick + 600);
                    domain.request_replan(tick + plan::REFUSED_REPLAN_TICKS);
                }
            }
            _ => domain.progress = Some((building, done, tick)),
        }
    }

    fn vassal_build(&mut self, id: EntityId, sites: &[plan::Site], work: &super::work::Workload) {
        let d = &self.economy.vassals.domains[&id];
        let (player, cursor, focus, anchor) = (d.player, d.cursor, d.status.focus, d.center);
        let threatened = d.hot.is_some();
        // Start near the castle while the road survey runs. Filling the entire
        // queue with distant walls first makes builders repeatedly cross the
        // domain before its local roads and production sites are available.
        let survey_pending = d.pending.is_some() && !threatened && d.paid.is_empty();
        let mut open = work.open.len();
        let mut wait = d.status.wait;
        if open >= PIPELINE {
            return;
        }
        let deficits = &work.deficits;
        let mut candidates: Vec<_> = deficits
            .iter()
            .copied()
            .filter_map(|(i, site)| {
                let occupied = self.building_at_for(player, site.tile).is_some();
                let replacement =
                    occupied && self.upgradable(player, site.tile, site.kind).is_some();
                let distance = super::super::placement::distance_squared(anchor, site.tile);
                let near = distance <= (VASSAL_RADIUS as u64 / 2).pow(2);
                ((!occupied || replacement)
                    && (!survey_pending || near || replacement)
                    && !d.refused.contains_key(&site.tile))
                .then_some((i, site, replacement, distance))
            })
            .collect();
        candidates.sort_by_key(|(i, site, replacement, distance)| {
            (
                site.build_priority(focus),
                if threatened { site.tier } else { 0 },
                !replacement,
                distance + u64::from(site.kind.rules().build_seconds) * 4,
                site.tier,
                *i < cursor,
                *i,
            )
        });
        let mut placed = BTreeSet::new();
        for (i, site, _, _) in candidates.into_iter().take(ATTEMPTS_PER_PASS * 4) {
            let plan::Site {
                kind,
                tile,
                vertical,
                ..
            } = site;
            if open >= PIPELINE || placed.len() >= ATTEMPTS_PER_PASS {
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
                        || open + run.len() >= PIPELINE
                        || *k != kind
                        || (t.x - last.x).abs() + (t.y - last.y).abs() != 1
                        || (run.len() > 1
                            && (t.x - last.x, t.y - last.y)
                                != (last.x - run[run.len() - 2].x, last.y - run[run.len() - 2].y))
                        || (self.building_at_for(player, *t).is_some()
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
            if !self.vassal_can_spend(player, cost, n) {
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
            self.record_vassal_decision();
            let result = if kind.is_wall() || kind == BuildingKind::Road {
                self.issue_build_wall(player, &[], kind, tile, run[run.len() - 1], false)
            } else if let Some(end) = site.end {
                self.issue_build_bridge(player, &[], tile, end, false)
                    .map(|id| vec![id])
            } else {
                self.issue_build_gate(player, &[], kind, tile, vertical, false)
                    .map(|id| vec![id])
            };
            if result.is_err() {
                for &(old, footprint) in &replaced {
                    let _ = self.insert_building(player, old, footprint, true);
                }
            }
            match result {
                Ok(ids) => {
                    self.queue_foundations(&ids, Some(id));
                    wait = VassalWait::Building;
                    open += run.len();
                    placed.extend(run);
                }
                Err(error) => {
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
