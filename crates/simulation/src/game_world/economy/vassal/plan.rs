//! The domain plan, cached on the domain and rebuilt only every
//! `REPLAN_TICKS`, on an age change or when it is empty: towers by the
//! villagers' work points, production behind the castle away from the enemy,
//! the domain wall ring (see `ring`) and the road to the Town Center.
use super::*;
use crate::Age;

#[cfg(test)]
#[path = "../../tests/economy/vassal/replan.rs"]
mod replan_tests;

/// A planned building. Lower `tier`s are built first.
#[derive(Clone, Copy, Debug)]
pub(super) struct Site {
    pub(super) kind: BuildingKind,
    /// Top-left tile (a bridge's near bank).
    pub(super) tile: TileCoord,
    /// A gate whose four tiles run along y.
    pub(super) vertical: bool,
    pub(super) tier: u8,
    /// A bridge's far bank.
    pub(super) end: Option<TileCoord>,
}

impl Site {
    pub(super) fn new(kind: BuildingKind, tile: TileCoord, tier: u8) -> Self {
        Self {
            kind,
            tile,
            vertical: false,
            tier,
            end: None,
        }
    }
}

/// Build order: the home towers, the threatened side's wall, then roads and
/// production, work-point towers, and the other sides by threat.
pub(super) const TIER_HOME_TOWER: u8 = 0;
pub(super) const TIER_ROAD: u8 = 2;
pub(super) const TIER_PRODUCTION: u8 = 2;
pub(super) const TIER_WORK_TOWER: u8 = 3;
/// One minute at 20 Hz: newly explored ground and new camps join the plan.
pub(super) const REPLAN_TICKS: u64 = 1_200;
/// A refused site is re-planned around within ten seconds.
pub(super) const REFUSED_REPLAN_TICKS: u64 = 200;
/// Work points (camps, mills, Town Centers) that each get a tower.
const WORK_TOWERS: usize = 8;

impl GameWorld {
    /// The cached plan of domain `id`, rebuilt when stale.
    pub(super) fn vassal_plan(&mut self, id: EntityId) -> Vec<Site> {
        let Some(d) = self.economy.vassals.domains.get(&id) else {
            return Vec::new();
        };
        let age = self.player_economy(d.player).map_or(Age::Dark, |p| p.age);
        let requested = d.replan_at.is_some_and(|at| self.tick.0 >= at);
        if !requested
            && (d.pending.is_some()
                || (!d.plan.is_empty()
                    && d.plan_age == age
                    && self.tick.0.saturating_sub(d.planned) < REPLAN_TICKS))
        {
            return d.plan.clone();
        }
        let mut plan = sites(self, d.center, d.player, age, &d.refused, d.hot);
        // Keep useful network orders while its replacement is being searched.
        plan.extend(
            d.plan
                .iter()
                .copied()
                .filter(|s| matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge)),
        );
        // Explicit fortification requests must not churn the retained network:
        // builders may already be laying its retained roads and bridge spans.
        let pending = if d.pending.is_none() && (!requested || d.plan.is_empty()) {
            super::road_job::Planner::new(self, d.center, d.player, &plan)
        } else {
            None
        };
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.plan.clone_from(&plan);
            d.plan_age = age;
            d.planned = self.tick.0;
            d.replan_at = None;
            if d.pending.is_none() {
                d.pending = pending;
            }
        }
        plan
    }
}

impl GameWorld {
    /// Footprints of the domain's planned fortification (every site but roads).
    #[cfg(test)]
    pub(crate) fn vassal_fortification(&mut self, id: EntityId) -> Vec<(BuildingKind, TileRect)> {
        self.finished_vassal_plan(id)
            .into_iter()
            .filter(|s| !matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge))
            .map(|s| {
                let r = s.kind.rules();
                let (w, h) = if s.vertical {
                    (r.depth, r.side)
                } else {
                    (r.side, r.depth)
                };
                (s.kind, TileRect::from_xywh(s.tile.x, s.tile.y, w, h))
            })
            .collect()
    }
}

impl GameWorld {
    /// Planned roads and bridges: kind, tile and a bridge's far bank.
    #[cfg(test)]
    pub(crate) fn vassal_network(
        &mut self,
        id: EntityId,
    ) -> Vec<(BuildingKind, TileCoord, Option<TileCoord>)> {
        self.finished_vassal_plan(id)
            .into_iter()
            .filter(|s| matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge))
            .map(|s| (s.kind, s.tile, s.end))
            .collect()
    }
}

fn footprint(kind: BuildingKind, t: TileCoord) -> TileRect {
    TileRect::from_xywh(t.x, t.y, kind.rules().side, kind.rules().depth)
}

fn center_of(r: TileRect) -> TileCoord {
    TileCoord::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2)
}

fn within(center: TileCoord, r: TileRect, reach: i32) -> bool {
    let c = center_of(r);
    (c.x - center.x).abs() <= reach && (c.y - center.y).abs() <= reach
}

/// First placeable tile on rings of growing radius around `at`, or `None`.
fn spot(
    w: &GameWorld,
    player: PlayerId,
    kind: BuildingKind,
    at: TileCoord,
    reach: i32,
    refused: &BTreeMap<TileCoord, u64>,
) -> Option<TileCoord> {
    (0..=reach).find_map(|r| {
        (-r..=r)
            .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| dx.abs() == r || dy.abs() == r)
            .map(|(dx, dy)| TileCoord::new(at.x + dx, at.y + dy))
            .find(|t| {
                !refused.contains_key(t)
                    && w.validate_building_placement(kind, *t).is_ok()
                    && w.validate_explored_footprint(player, footprint(kind, *t))
                        .is_ok()
            })
    })
}

/// Vector from the castle toward the threat: the last seen raider, else the
/// nearest enemy Town Center, else the map centre. Never zero.
pub(super) fn threat_vector(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    hot: Option<TileCoord>,
) -> (i32, i32) {
    let target = hot.unwrap_or_else(|| {
        w.players()
            .filter(|p| p.player != player)
            .flat_map(|p| w.player_buildings_of(p.player, BuildingKind::TownCenter))
            .filter(|b| b.hp > 0)
            .map(|b| center_of(b.footprint))
            .min_by_key(|c| {
                (
                    super::super::placement::distance_squared(*c, center),
                    c.y,
                    c.x,
                )
            })
            .unwrap_or(TileCoord::new(
                w.config.width_tiles / 2,
                w.config.height_tiles / 2,
            ))
    });
    match (target.x - center.x, target.y - center.y) {
        (0, 0) => (0, 1),
        v => v,
    }
}

pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    age: Age,
    refused: &BTreeMap<TileCoord, u64>,
    hot: Option<TileCoord>,
) -> Vec<Site> {
    let mut sites: Vec<Site> = Vec::new();
    let reach = super::ring::RING - 2;
    let toward = threat_vector(w, center, player, hot);
    let add = |sites: &mut Vec<Site>, kind: BuildingKind, at: TileCoord, near: i32, tier: u8| {
        // Production anywhere in the inner domain is reused, not duplicated.
        let reuse = if kind == BuildingKind::WatchTower {
            near
        } else {
            reach / 2
        };
        let home = if kind == BuildingKind::WatchTower {
            at
        } else {
            center
        };
        if let Some(b) = w
            .player_buildings_of(player, kind)
            .filter(|b| {
                b.hp > 0
                    && within(home, b.footprint, reuse)
                    && (b.is_complete() || !refused.contains_key(&b.footprint.min))
            })
            .min_by_key(|b| b.id)
        {
            sites.push(Site::new(kind, b.footprint.min, tier));
        } else if let Some(tile) = spot(w, player, kind, at, near, refused)
            .or_else(|| spot(w, player, kind, at, near * 3 + 3, refused))
        {
            sites.push(Site::new(kind, tile, tier));
        } else {
            // Nothing placeable nearby: the deficit stays (the domain is
            // blocked, not developed) until ground or a refusal clears.
            sites.push(Site::new(kind, at, tier));
        }
    };
    if age >= Age::Feudal {
        // Two towers by the castle come first.
        for (x, y) in [(5, -6), (-6, 5)] {
            add(
                &mut sites,
                BuildingKind::WatchTower,
                TileCoord::new(center.x + x, center.y + y),
                2,
                TIER_HOME_TOWER,
            );
        }
        // Towers guard the villagers where they work.
        let mut points: Vec<_> = [
            BuildingKind::TownCenter,
            BuildingKind::Mill,
            BuildingKind::LumberCamp,
            BuildingKind::MiningCamp,
        ]
        .into_iter()
        .flat_map(|k| w.player_buildings_of(player, k))
        .filter(|b| b.hp > 0 && within(center, b.footprint, reach))
        .map(|b| (b.id, b.footprint))
        .collect();
        points.sort_by_key(|(id, _)| *id);
        for (_, r) in points.into_iter().take(WORK_TOWERS) {
            let c = center_of(r);
            let at = TileCoord::new(c.x + r.width() / 2 + 2, c.y + r.height() / 2 + 2);
            add(&mut sites, BuildingKind::WatchTower, at, 3, TIER_WORK_TOWER);
        }
    }
    // Production behind the castle, away from the threat side.
    let (tx, ty) = (toward.0.signum(), toward.1.signum());
    let behind = TileCoord::new(center.x - tx * 10, center.y - ty * 10);
    add(
        &mut sites,
        BuildingKind::Barracks,
        behind,
        6,
        TIER_PRODUCTION,
    );
    if age >= Age::Feudal {
        for (kind, side) in [(BuildingKind::ArcheryRange, 1), (BuildingKind::Stable, -1)] {
            let at = if (tx, ty) == (0, 0) {
                TileCoord::new(behind.x + side * 6, behind.y)
            } else {
                TileCoord::new(behind.x + side * ty * 6, behind.y - side * tx * 6)
            };
            add(&mut sites, kind, at, 6, TIER_PRODUCTION);
        }
    }
    sites.extend(super::ring::sites(w, center, player, age, toward));
    sites
}

impl GameWorld {
    fn poll_vassal_plan(&mut self, id: EntityId) {
        let pending = self
            .economy
            .vassals
            .domains
            .get_mut(&id)
            .and_then(|d| d.pending.take());
        let Some(mut pending) = pending else {
            return;
        };
        let result = pending.poll(self);
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            if let Some(roads) = result {
                d.plan
                    .retain(|s| !matches!(s.kind, BuildingKind::Road | BuildingKind::Bridge));
                d.plan.extend(roads);
                d.planned = self.tick.0;
            } else {
                d.pending = Some(pending);
            }
        }
    }
    pub(super) fn advance_vassal_plans(&mut self) {
        use std::ops::Bound::{Excluded, Unbounded};
        let m = &self.economy.vassals;
        let cursor = m.plan_cursor.unwrap_or(EntityId(0));
        let next = m
            .domains
            .range((Excluded(cursor), Unbounded))
            .chain(m.domains.range(..=cursor))
            .find(|(_, d)| d.pending.is_some())
            .map(|(&id, _)| id);
        self.economy.vassals.plan_cursor = next;
        if let Some(id) = next {
            self.poll_vassal_plan(id);
        }
    }
    #[cfg(test)]
    pub(crate) fn vassal_planning_pending(&self, id: EntityId) -> bool {
        self.economy
            .vassals
            .domains
            .get(&id)
            .is_some_and(|d| d.pending.is_some())
    }
    #[cfg(test)]
    fn finished_vassal_plan(&mut self, id: EntityId) -> Vec<Site> {
        self.vassal_plan(id);
        while self.vassal_planning_pending(id) {
            self.poll_vassal_plan(id);
        }
        self.economy
            .vassals
            .domains
            .get(&id)
            .map(|d| d.plan.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tower_with_no_placeable_spot_stays_a_deficit() {
        let config = aoe_core::WorldConfig::new(256, 256, aoe_core::Seed(7)).unwrap();
        let mut w = GameWorld::new(config).unwrap();
        w.set_fog_disabled(true);
        w.place_town_center(PlayerId(0), TileCoord::new(64, 64))
            .unwrap();
        w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(90, 90, 4, 4),
            true,
        )
        .unwrap();
        // Every tile around the castle is refused: no tower can be placed.
        let refused: BTreeMap<TileCoord, u64> = (60..130)
            .flat_map(|y| (60..130).map(move |x| (TileCoord::new(x, y), u64::MAX)))
            .collect();
        let plan = sites(
            &w,
            TileCoord::new(92, 92),
            PlayerId(0),
            Age::Feudal,
            &refused,
            None,
        );
        assert_eq!(
            plan.iter()
                .filter(|s| s.kind == BuildingKind::WatchTower)
                .count(),
            3,
            "both castle towers and the Town Center's stay planned as deficits"
        );
    }
}
