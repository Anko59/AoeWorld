//! Building presentation decisions. The server remains authoritative.
use aoe_core::TileCoord;
use aoe_protocol::BuildingKind;
use aoe_protocol::Stockpile;

#[inline(never)]
pub fn rules(kind: BuildingKind) -> &'static aoe_simulation::BuildingRules {
    &aoe_simulation::BUILDING_RULES[kind as usize]
}
#[inline(never)]
pub fn stock(s: aoe_simulation::Stockpile) -> Stockpile {
    Stockpile {
        food: s.food,
        wood: s.wood,
        gold: s.gold,
        stone: s.stone,
    }
}

#[inline(never)]
pub fn affordable(have: Stockpile, need: Stockpile) -> bool {
    have.food >= need.food
        && have.wood >= need.wood
        && have.gold >= need.gold
        && have.stone >= need.stone
}

#[inline(never)]
pub fn build_enabled(kind: BuildingKind, stock: Stockpile, completed: u16) -> bool {
    let r = rules(kind);
    r.buildable
        && affordable(stock, self::stock(r.cost))
        && r.requires.is_none_or(|k| completed & (1 << k as u8) != 0)
}

#[inline(never)]
pub fn footprint_valid(
    kind: BuildingKind,
    origin: TileCoord,
    bounds: [i32; 2],
    mut free: impl FnMut(TileCoord) -> bool,
) -> bool {
    let side = rules(kind).side;
    if origin.x < 0
        || origin.y < 0
        || i64::from(origin.x) + i64::from(side) > i64::from(bounds[0])
        || i64::from(origin.y) + i64::from(side) > i64::from(bounds[1])
    {
        return false;
    }
    (origin.y..origin.y + side)
        .all(|y| (origin.x..origin.x + side).all(|x| free(TileCoord::new(x, y))))
}

/// Standing kinds, four foundation sizes, then rubble kinds.
#[inline(never)]
pub fn sprite_slot(kind: BuildingKind, progress: u16, rubble: bool) -> usize {
    if kind == BuildingKind::Dock {
        return if rubble {
            25
        } else if progress < 1000 {
            24
        } else {
            23
        };
    }
    if rubble {
        13 + kind as usize
    } else if progress < 1000 {
        9 + rules(kind).side as usize - 1
    } else {
        kind as usize
    }
}

#[inline(never)]
pub fn wall_tiles(from: TileCoord, to: TileCoord) -> Option<Vec<TileCoord>> {
    let (mut x, mut y) = (i64::from(from.x), i64::from(from.y));
    let (x1, y1) = (i64::from(to.x), i64::from(to.y));
    let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
    if dx.max(-dy) >= 64 {
        return None;
    }
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let mut error = dx + dy;
    let mut tiles = Vec::new();
    loop {
        tiles.push(TileCoord::new(x as i32, y as i32));
        if (x, y) == (x1, y1) {
            return Some(tiles);
        }
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests;

/// DE's five palisade silhouettes: /, backslash, junction, horizontal, vertical.
#[inline(never)]
pub fn wall_frame(at: TileCoord, mut occupied: impl FnMut(TileCoord) -> bool) -> u8 {
    let mut frame = None;
    for (dx, dy, next) in [
        (0, 1, 0),
        (0, -1, 0),
        (1, 0, 1),
        (-1, 0, 1),
        (1, 1, 4),
        (-1, -1, 4),
        (1, -1, 3),
        (-1, 1, 3),
    ] {
        if occupied(TileCoord::new(at.x + dx, at.y + dy)) {
            if frame.is_some_and(|old| old != next) {
                return 2;
            }
            frame = Some(next);
        }
    }
    frame.unwrap_or(2)
}
