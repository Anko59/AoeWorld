//! Rasterized bridge cells and their two permitted neighbours.
use super::{ResourceOverlay, ResourceOverlayError};
use aoe_core::{BridgeSpan, TileCoord, TileRect};

fn direction(from: TileCoord, to: TileCoord) -> u16 {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    if dx.abs().max(dy.abs()) != 1 {
        return 0;
    }
    1 << ((dy + 1) * 3 + dx + 1)
}

impl ResourceOverlay {
    /// Banks are ordinary ground; each deck tile connects only to the previous
    /// and next raster cell, including diagonal steps over otherwise deep water.
    pub fn bridge_edge(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        let a = self.bridge_connections(from);
        let b = self.bridge_connections(to);
        a.or(b)?;
        Some(
            a.is_none_or(|mask| mask & direction(from, to) != 0)
                && b.is_none_or(|mask| mask & direction(to, from) != 0),
        )
    }

    /// Reserve only the line, never unrelated cells inside its bounding box.
    pub fn add_bridge_foundation(&mut self, span: BridgeSpan) -> Result<(), ResourceOverlayError> {
        self.check_bridge_capacity(span, span.tiles().len())?;
        for tile in span.tiles() {
            self.add_footprint(TileRect::from_xywh(tile.x, tile.y, 1, 1))?;
        }
        Ok(())
    }

    pub fn add_bridge(&mut self, span: BridgeSpan) -> Result<(), ResourceOverlayError> {
        self.check_bridge_capacity(span, span.tiles().len().saturating_sub(2))?;
        self.toggle_span(span);
        Ok(())
    }

    fn check_bridge_capacity(
        &self,
        span: BridgeSpan,
        count: usize,
    ) -> Result<(), ResourceOverlayError> {
        if !span.valid()
            || span
                .tiles()
                .iter()
                .any(|t| self.bridge_connections(*t).is_some())
        {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        if (self.bridge_tile_count() as u64)
            .saturating_add(self.navigation_state.obstacles.len() as u64)
            .saturating_add(count as u64)
            > super::MAX_OBSTACLE_TILES
        {
            return Err(ResourceOverlayError::Capacity);
        }
        Ok(())
    }

    pub fn complete_bridge(&mut self, span: BridgeSpan) -> bool {
        if !span.valid()
            || span.tiles().iter().any(|t| {
                !self.navigation_state.obstacles.contains(t)
                    || self.bridge_connections(*t).is_some()
            })
        {
            return false;
        }
        self.remove_bridge_foundation(span);
        self.toggle_span(span);
        true
    }

    pub fn remove_bridge_foundation(&mut self, span: BridgeSpan) {
        for tile in span.tiles() {
            self.remove_footprint(TileRect::from_xywh(tile.x, tile.y, 1, 1));
        }
    }

    pub fn remove_bridge(&mut self, span: BridgeSpan) {
        let tiles = span.tiles();
        for triple in tiles.windows(3) {
            let tile = triple[1];
            let mask = direction(tile, triple[0]) | direction(tile, triple[2]);
            if self.bridge_connections(tile) == Some(mask) {
                self.navigation_toggle(tile, 4, u64::from(mask));
            }
        }
    }

    fn toggle_span(&mut self, span: BridgeSpan) {
        for triple in span.tiles().windows(3) {
            let tile = triple[1];
            let mask = direction(tile, triple[0]) | direction(tile, triple[2]);
            self.navigation_toggle(tile, 4, u64::from(mask));
        }
    }
}
