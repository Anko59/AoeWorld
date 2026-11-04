//! Bounded local road route, through friendly gates to the nearest Town Center.
use super::super::placement::{distance_squared, perimeter};
use super::*;
use std::collections::VecDeque;

pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
) -> Vec<(BuildingKind, TileCoord)> {
    let Some(castle) = w
        .player_buildings_of(player, BuildingKind::Castle)
        .find(|b| b.footprint.contains(center))
    else {
        return Vec::new();
    };
    let Some(town) = w
        .player_buildings_of(player, BuildingKind::TownCenter)
        .filter(|b| b.hp > 0 && b.is_complete())
        .min_by_key(|b| {
            (
                distance_squared(
                    center,
                    TileCoord::new(
                        (b.footprint.min.x + b.footprint.max.x) / 2,
                        (b.footprint.min.y + b.footprint.max.y) / 2,
                    ),
                ),
                b.id,
            )
        })
    else {
        return vec![(BuildingKind::Road, center)];
    };
    let goals: BTreeSet<_> = perimeter(town.footprint).into_iter().collect();
    let passable = |tile| {
        w.building_at(tile)
            .and_then(|id| w.building(id))
            .map_or_else(
                || w.tile_passable(tile),
                |b| {
                    b.player == player
                        && (b.kind == BuildingKind::Road
                            || (b.kind.is_gate() || b.kind == BuildingKind::Bridge)
                                && b.is_complete())
                },
            )
    };
    let mut starts = perimeter(castle.footprint);
    starts.sort_by_key(|tile| (distance_squared(*tile, town.footprint.min), tile.y, tile.x));
    let mut parents = BTreeMap::new();
    let mut queue = VecDeque::new();
    for tile in starts.into_iter().filter(|t| passable(*t)) {
        parents.insert(tile, tile);
        queue.push_back(tile);
    }
    // At most 8192 tiles per domain pass, independent of world dimensions.
    while let Some(tile) = queue.pop_front() {
        if goals.contains(&tile) {
            let mut route = Vec::new();
            let mut at = tile;
            loop {
                if w.building_at(at)
                    .and_then(|id| w.building(id))
                    .is_none_or(|b| b.kind == BuildingKind::Road)
                {
                    route.push((BuildingKind::Road, at));
                }
                let previous = parents[&at];
                if previous == at {
                    break;
                }
                at = previous;
            }
            route.reverse();
            return route;
        }
        if parents.len() >= 8192 {
            break;
        }
        let mut next =
            [(0, -1), (-1, 0), (1, 0), (0, 1)].map(|(x, y)| TileCoord::new(tile.x + x, tile.y + y));
        next.sort_by_key(|t| (distance_squared(*t, town.footprint.min), t.y, t.x));
        for at in next {
            if !parents.contains_key(&at) && passable(at) && w.terrain.crossable(tile, at, w.config)
            {
                parents.insert(at, tile);
                queue.push_back(at);
            }
        }
    }
    // An unavailable route is a real deficit, never a completed domain.
    vec![(BuildingKind::Road, center)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn road_plan_chooses_the_nearest_completed_owned_town_center() {
        let config = aoe_core::WorldConfig::new(256, 256, aoe_core::Seed(7)).unwrap();
        let mut w = GameWorld::new(config).unwrap();
        w.place_town_center(PlayerId(0), TileCoord::new(64, 64))
            .unwrap();
        let nearest = w
            .place_town_center(PlayerId(0), TileCoord::new(110, 90))
            .unwrap();
        w.place_town_center(PlayerId(1), TileCoord::new(96, 90))
            .unwrap();
        w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(90, 90, 4, 4),
            true,
        )
        .unwrap();
        let route = sites(&w, TileCoord::new(92, 92), PlayerId(0));
        assert!(route.len() < 30);
        assert!(
            perimeter(w.building(nearest).unwrap().footprint).contains(&route.last().unwrap().1)
        );
        w.remove_building(nearest);
        let route = sites(&w, TileCoord::new(92, 92), PlayerId(0));
        assert!(route.last().unwrap().1.x < 70);
    }
}
