//! The domain plan, cached on the domain and rebuilt only every
//! `REPLAN_TICKS`, on an age change or when it is empty: towers by the
//! villagers' work points, production behind the castle away from the enemy,
//! the domain wall ring (see `ring`) and the road to the Town Center.
use super::*;
use crate::Age;

/// A planned building: kind, top-left tile and, for a gate, whether its
/// four tiles run along y.
pub(super) type Site = (BuildingKind, TileCoord, bool);
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
        if !d.plan.is_empty()
            && d.plan_age == age
            && self.tick.0.saturating_sub(d.planned) < REPLAN_TICKS
        {
            return d.plan.clone();
        }
        let plan = sites(self, d.center, d.player, age, &d.refused);
        if let Some(d) = self.economy.vassals.domains.get_mut(&id) {
            d.plan.clone_from(&plan);
            d.plan_age = age;
            d.planned = self.tick.0;
        }
        plan
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

/// Unit step from the castle toward the nearest enemy Town Center, or toward
/// the map centre when no enemy is known. Production stands on the far side.
fn threat_side(w: &GameWorld, center: TileCoord, player: PlayerId) -> (i32, i32) {
    let enemy = w
        .buildings()
        .filter(|b| b.player != player && b.kind == BuildingKind::TownCenter && b.hp > 0)
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
        ));
    ((enemy.x - center.x).signum(), (enemy.y - center.y).signum())
}

pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    age: Age,
    refused: &BTreeMap<TileCoord, u64>,
) -> Vec<Site> {
    let mut sites = Vec::new();
    let reach = super::ring::RING - 2;
    let add = |sites: &mut Vec<Site>, kind: BuildingKind, at: TileCoord, near: i32| {
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
            .filter(|b| b.hp > 0 && within(home, b.footprint, reuse))
            .min_by_key(|b| b.id)
        {
            sites.push((kind, b.footprint.min, false));
        } else if let Some(tile) = spot(w, player, kind, at, near, refused) {
            sites.push((kind, tile, false));
        }
    };
    if age >= Age::Feudal {
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
            add(&mut sites, BuildingKind::WatchTower, at, 3);
        }
        for (x, y) in [(5, -6), (-6, 5)] {
            add(
                &mut sites,
                BuildingKind::WatchTower,
                TileCoord::new(center.x + x, center.y + y),
                2,
            );
        }
    }
    // Production behind the castle, away from the threat side.
    let (tx, ty) = threat_side(w, center, player);
    let behind = TileCoord::new(center.x - tx * 10, center.y - ty * 10);
    add(&mut sites, BuildingKind::Barracks, behind, 6);
    if age >= Age::Feudal {
        for (kind, side) in [(BuildingKind::ArcheryRange, 1), (BuildingKind::Stable, -1)] {
            let at = TileCoord::new(behind.x + side * ty * 6, behind.y - side * tx * 6);
            let at = if (tx, ty) == (0, 0) {
                TileCoord::new(behind.x + side * 6, behind.y)
            } else {
                at
            };
            add(&mut sites, kind, at, 6);
        }
    }
    sites.extend(super::ring::sites(w, center, player, age));
    sites.extend(
        super::roads::sites(w, center, player)
            .into_iter()
            .map(|(k, t)| (k, t, false)),
    );
    sites
}
