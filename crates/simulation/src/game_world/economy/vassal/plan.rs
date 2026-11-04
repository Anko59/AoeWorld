use super::*;
use crate::Age;

pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    age: Age,
    refused: &BTreeMap<TileCoord, u64>,
) -> Vec<(BuildingKind, TileCoord)> {
    let mut sites = Vec::new();
    let mut add_buildings = |kind: BuildingKind, count, offsets: &[(i32, i32)]| {
        let mut selected: Vec<_> = w
            .player_buildings_of(player, kind)
            .filter(|b| {
                b.hp > 0
                    && b.footprint.min.x >= center.x - 8
                    && b.footprint.max.x <= center.x + 9
                    && b.footprint.min.y >= center.y - 8
                    && b.footprint.max.y <= center.y + 9
            })
            .take(count)
            .map(|b| b.footprint.min)
            .collect();
        for &(x, y) in offsets {
            if selected.len() == count {
                break;
            }
            let tile = TileCoord::new(center.x + x, center.y + y);
            if !selected.contains(&tile)
                && !refused.contains_key(&tile)
                && w.validate_building_placement(kind, tile).is_ok()
            {
                selected.push(tile);
            }
        }
        // Keep an explicit deficit when every alternative is blocked. It must
        // not count as developed or silently forget the missing fortification.
        for &(x, y) in offsets {
            if selected.len() == count {
                break;
            }
            let tile = TileCoord::new(center.x + x, center.y + y);
            if !selected.contains(&tile) {
                selected.push(tile);
            }
        }
        sites.extend(selected.into_iter().map(|tile| (kind, tile)));
    };
    if age >= Age::Feudal {
        add_buildings(
            BuildingKind::WatchTower,
            2,
            &[
                (-6, -6),
                (5, 5),
                (-6, 5),
                (5, -6),
                (-6, 0),
                (0, -6),
                (0, 5),
                (5, 0),
            ],
        );
    }
    add_buildings(
        BuildingKind::Barracks,
        1,
        &[(4, -4), (-5, 3), (-5, -4), (3, 3)],
    );
    let (wall, gate) = if age >= Age::Castle {
        (BuildingKind::StoneWall, BuildingKind::StoneGate)
    } else {
        (BuildingKind::PalisadeWall, BuildingKind::PalisadeGate)
    };
    let mut add = |kind: BuildingKind, x, y| {
        let tile = TileCoord::new(center.x + x, center.y + y);
        let kind = w
            .building_at(tile)
            .and_then(|id| w.building(id))
            .filter(|b| {
                b.player == player
                    && ((kind.is_wall() && b.kind.is_wall())
                        || (kind.is_gate() && b.kind.is_gate()))
            })
            .map_or(kind, |b| b.kind);
        sites.push((kind, tile));
    };
    for y in [-10, 10] {
        add(gate, -2, y);
    }
    for y in [-10, 10] {
        for x in -10..=10 {
            if !(-2..2).contains(&x) {
                add(wall, x, y);
            }
        }
    }
    for y in -9..10 {
        for x in [-10, 10] {
            add(wall, x, y);
        }
    }
    sites.extend(super::roads::sites(w, center, player));
    sites
}
