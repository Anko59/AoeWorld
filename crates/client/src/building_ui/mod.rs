//! Building presentation decisions. The server remains authoritative.
use aoe_core::TileCoord;
use aoe_protocol::BuildingKind;
use aoe_protocol::Stockpile;

pub fn gate_sprite_slot(open: bool, progress: u16) -> Option<u8> {
    (open && progress == 1000).then_some(42)
}

/// Only visible gates with changed authoritative art inputs are recomputed.
#[derive(Default)]
pub(crate) struct GateArt {
    entries: std::collections::BTreeMap<aoe_core::EntityId, (bool, u16, Option<u8>)>,
    updates: u64,
}
impl GateArt {
    pub fn observe(
        &mut self,
        id: aoe_core::EntityId,
        open: bool,
        progress: u16,
        visible: bool,
    ) -> Option<u8> {
        if !visible {
            return None;
        }
        if let Some(&(old_open, old_progress, slot)) = self.entries.get(&id)
            && (old_open, old_progress) == (open, progress)
        {
            return slot;
        }
        if self.entries.len() == aoe_protocol::MAX_ECONOMY_BUILDINGS
            && !self.entries.contains_key(&id)
        {
            self.entries.clear();
        }
        let slot = gate_sprite_slot(open, progress);
        self.entries.insert(id, (open, progress, slot));
        self.updates += 1;
        slot
    }
    #[cfg(test)]
    pub fn updates(&self) -> u64 {
        self.updates
    }
}

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

/// Market trade panel numbers: three gold prices, then three buy and three
/// sell enable flags for wood, food and stone. Buttons stay disabled unless
/// the selected building is the owner's completed Market; the server decides.
#[inline(never)]
pub fn market_panel(
    market_owned_complete: bool,
    prices: aoe_protocol::MarketPrices,
    have: Stockpile,
) -> [f64; 9] {
    let rules = aoe_simulation::MARKET_RULES;
    let mut panel = [0.0; 9];
    for (i, (resource, price)) in [
        (have.wood, prices.wood),
        (have.food, prices.food),
        (have.stone, prices.stone),
    ]
    .into_iter()
    .enumerate()
    {
        panel[i] = f64::from(price);
        let buy = have.gold >= rules.buy_cost(price);
        let sell = resource >= rules.trade_amount;
        panel[3 + i] = f64::from(u8::from(market_owned_complete && buy));
        panel[6 + i] = f64::from(u8::from(market_owned_complete && sell));
    }
    panel
}

#[inline(never)]
pub fn build_enabled(
    kind: BuildingKind,
    stock: Stockpile,
    completed: u16,
    age: aoe_protocol::Age,
) -> bool {
    let r = rules(kind);
    r.buildable
        && (r.age == aoe_simulation::Age::Dark || age == aoe_protocol::Age::Feudal)
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
        || i64::from(origin.y) + i64::from(rules(kind).depth) > i64::from(bounds[1])
    {
        return false;
    }
    (origin.y..origin.y + rules(kind).depth)
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
    if kind == BuildingKind::PalisadeGate && progress < 1000 && !rubble {
        43
    } else if rubble {
        if kind as usize <= 8 {
            13 + kind as usize
        } else {
            22 + kind as usize
        }
    } else if progress < 1000 {
        9 + rules(kind).side as usize - 1
    } else {
        if kind as usize <= 8 {
            kind as usize
        } else {
            16 + kind as usize
        }
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

/// Age choices share the dat's architectural families; camps already use age2
/// art in Dark Age because DE has no age1 camp sprites.
#[inline(never)]
pub fn age_sprite_slot(
    kind: BuildingKind,
    progress: u16,
    rubble: bool,
    age: aoe_protocol::Age,
) -> usize {
    const FEUDAL: [u8; 16] = [38, 39, 2, 3, 40, 5, 41, 7, 8, 44, 26, 27, 28, 29, 30, 31];
    if progress == 1000 && !rubble && age == aoe_protocol::Age::Feudal {
        usize::from(FEUDAL[kind as usize])
    } else {
        sprite_slot(kind, progress, rubble)
    }
}
