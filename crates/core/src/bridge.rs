//! Canonical single-lane bridge geometry shared by placement and navigation.
use crate::{TileCoord, TileRect, WORLD_LIMIT};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeSpan {
    pub from: TileCoord,
    pub to: TileCoord,
}

impl BridgeSpan {
    pub fn new(from: TileCoord, to: TileCoord) -> Option<Self> {
        let distance = from.x.abs_diff(to.x).max(from.y.abs_diff(to.y));
        if !(2..=41).contains(&distance)
            || [from.x, from.y, to.x, to.y]
                .iter()
                .any(|v| !(0..WORLD_LIMIT).contains(v))
        {
            return None;
        }
        let (from, to) = if (from.x, from.y) <= (to.x, to.y) {
            (from, to)
        } else {
            (to, from)
        };
        Some(Self { from, to })
    }

    pub fn valid(self) -> bool {
        Self::new(self.from, self.to) == Some(self)
    }

    pub fn footprint(self) -> TileRect {
        TileRect::new(
            TileCoord::new(self.from.x.min(self.to.x), self.from.y.min(self.to.y)),
            TileCoord::new(
                self.from.x.max(self.to.x).saturating_add(1),
                self.from.y.max(self.to.y).saturating_add(1),
            ),
        )
    }

    /// Integer rasterization is independent of drag direction and never creates
    /// disconnected cells. Even malformed decoded values keep allocation bounded.
    pub fn tiles(self) -> Vec<TileCoord> {
        raster_bridge_line(self.from, self.to).unwrap_or_default()
    }

    pub fn contains(self, tile: TileCoord) -> bool {
        if !self.footprint().contains(tile) {
            return false;
        }
        let dx = i64::from(self.to.x) - i64::from(self.from.x);
        let dy = i64::from(self.to.y) - i64::from(self.from.y);
        let steps = dx.abs().max(dy.abs());
        if !(1..64).contains(&steps) {
            return tile == self.from && steps == 0;
        }
        let index = if dx.abs() >= dy.abs() {
            i64::from(tile.x) - i64::from(self.from.x)
        } else {
            (i64::from(tile.y) - i64::from(self.from.y)) * dy.signum()
        };
        let offset = |delta: i64| (delta * index + delta.signum() * (steps / 2)) / steps;
        i64::from(tile.x) == i64::from(self.from.x) + offset(dx)
            && i64::from(tile.y) == i64::from(self.from.y) + offset(dy)
    }
}

/// Preview includes stationary and overlong (invalid) spans, bounded to 64 cells.
/// Actual placement still requires `BridgeSpan::new`.
pub fn raster_bridge_line(from: TileCoord, to: TileCoord) -> Option<Vec<TileCoord>> {
    let (from, to) = if (from.x, from.y) <= (to.x, to.y) {
        (from, to)
    } else {
        (to, from)
    };
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let steps = dx.abs().max(dy.abs());
    if steps >= 64 {
        return None;
    }
    if steps == 0 {
        return Some(vec![from]);
    }
    Some(
        (0..=steps)
            .map(|i| {
                let offset =
                    |delta: i64| ((delta * i + delta.signum() * (steps / 2)) / steps) as i32;
                TileCoord::new(from.x + offset(dx), from.y + offset(dy))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;

/// Keep an exact valid drag or an existing land-to-water entry. Only a missing
/// entry permits searching the eight neighbouring starts, never repairing an end.
/// The caller supplies its authoritative terrain, occupancy and knowledge rules.
pub fn snap_bridge_start(
    from: TileCoord,
    to: TileCoord,
    water: impl Fn(TileCoord) -> Option<bool>,
    valid: impl Fn(BridgeSpan) -> bool,
) -> Option<BridgeSpan> {
    if let Some(span) = BridgeSpan::new(from, to).filter(|span| valid(*span)) {
        return Some(span);
    }
    let mut tiles = raster_bridge_line(from, to)?;
    if tiles.first() != Some(&from) {
        tiles.reverse();
    }
    if water(from) == Some(false) && tiles.get(1).is_some_and(|tile| water(*tile) == Some(true)) {
        return None;
    }
    let mut starts = Vec::with_capacity(9);
    for dy in -1_i32..=1 {
        for dx in -1_i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            starts.push((
                dx * dx + dy * dy,
                from.y.saturating_add(dy),
                from.x.saturating_add(dx),
            ));
        }
    }
    starts.sort_unstable();
    starts
        .into_iter()
        .find_map(|(_, y, x)| BridgeSpan::new(TileCoord::new(x, y), to).filter(|span| valid(*span)))
}
