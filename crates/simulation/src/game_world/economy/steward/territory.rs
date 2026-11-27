//! Steward land ownership follows walking costs. Water work keeps its own
//! shoreline service area so a land barrier does not strand fishing boats.
use super::{Home, STEWARD_RULES};
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, PlayerId, TileCoord, TileRect, WorldPosition};
use std::sync::Arc;

fn d2(a: WorldPosition, b: WorldPosition) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}
impl Home {
    /// Inside the circle and not closer to (or tied but lower-ID than) another
    /// active town_center of the same player.
    pub(crate) fn owns(&self, at: WorldPosition) -> bool {
        self.walking.as_ref().map_or_else(
            || self.owns_water(at),
            |walking| walking.owns(self.id, at.tile_floor()),
        )
    }

    pub(crate) fn owns_water(&self, at: WorldPosition) -> bool {
        self.contains(at) && {
            let mine = d2(self.center, at);
            !self.rivals.iter().any(|&(id, c)| {
                let theirs = d2(c, at);
                theirs < mine || (theirs == mine && id < self.id)
            })
        }
    }

    pub(super) fn owns_resident(&self, kind: UnitKind, at: WorldPosition) -> bool {
        if kind == UnitKind::FishingShip {
            self.owns_water(at)
        } else {
            self.owns(at)
        }
    }

    /// Every margin-expanded corner fits the service area, and the site's
    /// centre belongs to this office. Docks can be centred over water.
    pub(in super::super) fn covers(&self, rect: TileRect, margin: i32) -> bool {
        self.covers_site(rect, margin, false)
    }

    pub(super) fn covers_shore(&self, rect: TileRect, margin: i32) -> bool {
        self.covers_site(rect, margin, true)
    }

    fn covers_site(&self, rect: TileRect, margin: i32, shore: bool) -> bool {
        let center = WorldPosition::new(
            (rect.min.x + rect.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
            (rect.min.y + rect.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
        );
        let owns = if shore {
            self.owns_water(center)
        } else {
            self.owns(center)
        };
        owns && [rect.min.x - margin, rect.max.x + margin]
            .into_iter()
            .all(|x| {
                [rect.min.y - margin, rect.max.y + margin]
                    .into_iter()
                    .all(|y| {
                        self.contains(WorldPosition::new(
                            x * FIXED_SUBUNITS_PER_TILE,
                            y * FIXED_SUBUNITS_PER_TILE,
                        ))
                    })
            })
    }
}
impl GameWorld {
    pub(super) fn refresh_steward_walk(&mut self) {
        let period = self.config.tick_hz;
        for key in self.economy.walking.stewards.due(self.tick.0, period) {
            let cache = &self.economy.walking;
            let Some(walking) = cache.stewards.refresh(self, key, cache) else {
                continue;
            };
            for id in walking.offices() {
                if let Some(home) = self.economy.steward_homes.get_mut(&id) {
                    if home.walking.is_none() || walking.zone_changed(id) {
                        home.walk_recheck = true;
                        home.regions = None;
                        // The zone moved: sweep the buildings and units it
                        // now covers, from the start.
                        home.restart_sweeps();
                        self.economy
                            .stewards
                            .controlled
                            .mark(home.workers.iter().copied());
                    }
                    home.walking = Some(walking.clone());
                }
            }
            self.economy.walking.stewards.publish(key, walking);
        }
    }

    pub(super) fn recheck_steward_walk(&mut self, id: EntityId, changed: &mut Vec<EntityId>) {
        let Some(home) = self.economy.steward_homes.get_mut(&id) else {
            return;
        };
        if !std::mem::take(&mut home.walk_recheck) {
            return;
        }
        let player = home.player;
        let workers = home
            .workers
            .iter()
            .copied()
            .filter(|worker| {
                self.economy.gatherers.get(worker).is_some_and(|job| {
                    job.farm.is_none()
                        && home
                            .walking
                            .as_ref()
                            .and_then(|zone| zone.patch_owner(job.tile))
                            .is_some_and(|owner| owner != id)
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            self.record_order_decisions(1);
            if self.issue_stop(player, worker).is_ok() {
                changed.push(worker);
            }
        }
    }

    /// Recompute each active domain's nearby rivals for one player. Runs only
    /// when a domain activates or deactivates, never per tick.
    pub(super) fn refresh_steward_territory(&mut self, player: PlayerId) {
        let members: Vec<(EntityId, WorldPosition)> = self
            .economy
            .stewards
            .by_player
            .get(&player)
            .map(|m| m.iter().map(|(id, at)| (*id, *at)).collect())
            .unwrap_or_default();
        let reach = i64::from(2 * STEWARD_RULES.radius_tiles * FIXED_SUBUNITS_PER_TILE);
        for &(id, center) in &members {
            let rivals = members
                .iter()
                .filter(|&&(other, at)| other != id && d2(center, at) <= reach * reach)
                .copied()
                .collect();
            if let Some(home) = self.economy.steward_homes.get_mut(&id) {
                self.economy
                    .stewards
                    .controlled
                    .mark(home.workers.iter().chain(&home.navy.ships).copied());
                home.rivals = rivals;
            }
        }
    }
}

// Connectivity is clipped to the office's territory using the same region
// steps as its walking field. Passing through another office is disallowed.
#[derive(Debug)]
pub(super) struct Regions {
    bounds: TileRect,
    // Zero means closed or outside the walking zone.
    labels: Vec<u32>,
}

impl Regions {
    fn label(&self, tile: TileCoord) -> u32 {
        if !self.bounds.contains(tile) {
            return 0;
        }
        let (x, y) = (tile.x - self.bounds.min.x, tile.y - self.bounds.min.y);
        self.labels[(y * self.bounds.width() + x) as usize]
    }

    fn label_near(&self, tile: TileCoord) -> u32 {
        for r in 0..=1 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let label = self.label(TileCoord::new(tile.x + dx, tile.y + dy));
                    if label != 0 {
                        return label;
                    }
                }
            }
        }
        0
    }

    /// Can a walker at `from` reach the tiles touching `target` without
    /// leaving the territory? Unknown starting ground is allowed.
    pub(super) fn connects(&self, from: TileCoord, target: TileRect) -> bool {
        let start = self.label_near(from);
        if start == 0 {
            return true;
        }
        (target.min.y - 1..=target.max.y).any(|y| {
            (target.min.x - 1..=target.max.x).any(|x| self.label(TileCoord::new(x, y)) == start)
        })
    }
}

impl Home {
    pub(in super::super) fn connects(&self, from: TileCoord, target: TileRect) -> bool {
        self.regions
            .as_ref()
            .is_none_or(|regions| regions.connects(from, target))
    }
    pub(super) fn connects_tile(&self, from: TileCoord, target: TileCoord) -> bool {
        self.connects(
            from,
            TileRect::new(target, TileCoord::new(target.x + 1, target.y + 1)),
        )
    }
}

impl GameWorld {
    /// Labels every domain of this pass whose labels are stale, each on its
    /// own task: labels are a pure function of the domain's walking zone, and
    /// zones change only before the pass (`refresh_steward_walk`), so a visit
    /// finds exactly the labels it would have built itself (#978).
    pub(super) fn steward_prepare_regions(&mut self, ids: &[EntityId]) {
        let mut stale: Vec<_> = ids
            .iter()
            .filter_map(|&id| {
                let home = self.economy.steward_homes.get(&id)?;
                let zone = home.walking.clone().filter(|_| home.regions.is_none())?;
                Some((id, zone, None))
            })
            .collect();
        crate::game_world::parallel::par_each_mut(&mut stale, |(id, zone, labels)| {
            *labels = zone.regions(*id);
        });
        for (id, _, labels) in stale {
            if let (Some(home), Some((bounds, labels))) =
                (self.economy.steward_homes.get_mut(&id), labels)
            {
                home.regions = Some(Arc::new(Regions { bounds, labels }));
            }
        }
    }

    pub(super) fn steward_ensure_regions(&mut self, id: EntityId) {
        let Some(home) = self.economy.steward_homes.get_mut(&id) else {
            return;
        };
        if home.regions.is_none()
            && let Some((bounds, labels)) = home.walking.as_ref().and_then(|zone| zone.regions(id))
        {
            home.regions = Some(Arc::new(Regions { bounds, labels }));
        }
    }
}

#[cfg(test)]
impl GameWorld {
    /// Test hook: label every tile within `margin` of `rect` as its own
    /// component, or restore ordinary labelling when `rect` is `None`.
    pub(crate) fn steward_cut_territory(&mut self, id: EntityId, rect: Option<TileRect>) {
        self.steward_ensure_regions(id);
        let Some(home) = self.economy.steward_homes.get_mut(&id) else {
            return;
        };
        let Some(rect) = rect else {
            home.regions = None;
            return;
        };
        let Some(old) = home.regions.as_ref() else {
            return;
        };
        let mut labels = old.labels.clone();
        let width = old.bounds.width();
        for y in 0..old.bounds.height() {
            for x in 0..width {
                let (tx, ty) = (old.bounds.min.x + x, old.bounds.min.y + y);
                let near = tx >= rect.min.x - 6
                    && tx <= rect.max.x + 6
                    && ty >= rect.min.y - 6
                    && ty <= rect.max.y + 6;
                if near && labels[(y * width + x) as usize] != 0 {
                    labels[(y * width + x) as usize] = u32::MAX;
                }
            }
        }
        home.regions = Some(Arc::new(Regions {
            bounds: old.bounds,
            labels,
        }));
    }
}

impl GameWorld {
    pub(crate) fn steward_land_sources(&self) -> Vec<(PlayerId, aoe_core::TileCoord, i32)> {
        self.economy
            .steward_homes
            .values()
            .filter(|h| h.town_center.is_some())
            .map(|h| (h.player, h.center.tile_floor(), STEWARD_RULES.radius_tiles))
            .collect()
    }
}
