//! One variable-length raster bridge with walkable land banks.
use crate::{
    BuildingKind, EconomyError, GameBuilding, GameWorld, GameWorldError, Stockpile, Terrain,
};
use aoe_core::{BridgeSpan, EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::WaterKind;

impl GameBuilding {
    pub fn contains_tile(&self, tile: TileCoord) -> bool {
        self.bridge_span
            .map_or_else(|| self.footprint.contains(tile), |span| span.contains(tile))
    }

    /// Bridges pay per water tile; all other buildings pay once.
    pub(super) fn construction_cost(&self) -> Stockpile {
        let count = if self.kind == BuildingKind::Bridge {
            (self.footprint.width().max(self.footprint.height()) - 2) as u32
        } else {
            1
        };
        let cost = self.kind.rules().cost;
        Stockpile {
            food: cost.food * count,
            wood: cost.wood * count,
            gold: cost.gold * count,
            stone: cost.stone * count,
        }
    }
}

impl GameWorld {
    /// Validate the complete raster span before reserving cells or charging.
    pub fn issue_build_bridge(
        &mut self,
        player: PlayerId,
        builders: &[EntityId],
        from: TileCoord,
        to: TileCoord,
        queued: bool,
    ) -> Result<EntityId, EconomyError> {
        let builders = self.validate_builders(player, builders)?;
        self.validate_build_queue(&builders, 1, queued)?;
        let span = BridgeSpan::new(from, to).ok_or(EconomyError::InvalidFootprint)?;
        let rect = span.footprint();
        let cost =
            self.check_buildable(player, BuildingKind::Bridge, span.tiles().len() as u32 - 2)?;
        for tile in span.tiles() {
            self.validate_explored_footprint(player, TileRect::from_xywh(tile.x, tile.y, 1, 1))?;
        }
        self.validate_bridge_placement(span)?;
        let id =
            self.insert_building_shape(player, BuildingKind::Bridge, rect, false, Some(span))?;
        self.charge(player, cost);
        if !self.assign_build_orders(&builders, &[id], queued) {
            let _ = self.issue_cancel_construction(player, id);
            return Err(GameWorldError::Unreachable.into());
        }
        self.remember_visible_buildings();
        Ok(id)
    }

    pub fn bridge_footprint(from: TileCoord, to: TileCoord) -> Option<TileRect> {
        Some(BridgeSpan::new(from, to)?.footprint())
    }

    fn validate_bridge_placement(&self, span: BridgeSpan) -> Result<(), EconomyError> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Err(EconomyError::InvalidFootprint);
        };
        if !span
            .footprint()
            .valid(self.config.width_tiles, self.config.height_tiles)
            || self
                .query(span.footprint())
                .0
                .iter()
                .any(|u| u.hp > 0 && !self.embarked(u.id) && span.contains(u.position.tile_floor()))
        {
            return Err(EconomyError::InvalidFootprint);
        }
        for tile in span.tiles() {
            let end = tile == span.from || tile == span.to;
            let (sample, node) = generator
                .tile_and_node_with_cancel(tile, &|| false)
                .map_err(GameWorldError::Environment)?
                .ok_or(EconomyError::InvalidFootprint)?;
            if overlay.blocks_tile(tile)
                || overlay.bridge_connections(tile).is_some()
                || node.is_some_and(|n| overlay.blocks_node(n))
                || self.buildings().any(|b| b.contains_tile(tile))
                || if end {
                    !sample.passable || sample.water != WaterKind::None
                } else {
                    sample.water == WaterKind::None
                }
            {
                return Err(EconomyError::InvalidFootprint);
            }
        }
        Ok(())
    }

    pub(super) fn complete_bridge(&mut self, span: BridgeSpan) -> bool {
        let Terrain::Map { overlay, .. } = &mut self.terrain else {
            return false;
        };
        if !overlay.complete_bridge(span) {
            return false;
        }
        for tile in span.tiles() {
            self.economy.blocked_tiles.remove(&tile);
        }
        self.collision_opened(span.footprint());
        true
    }

    pub(super) fn remove_bridge_deck(&mut self, span: BridgeSpan) {
        // Only occupants over underlying blocking water drown. Endpoints and
        // naturally wadeable marsh remain safe when the structure collapses.
        let drowned = self
            .query(span.footprint())
            .0
            .into_iter()
            .filter(|u| {
                let tile = u.position.tile_floor();
                u.hp > 0
                    && !u.kind.is_naval()
                    && tile != span.from
                    && tile != span.to
                    && span.contains(tile)
                    && matches!(&self.terrain, Terrain::Map { generator, .. }
                    if generator.tile_at(tile).is_some_and(|t| !t.passable))
            })
            .map(|u| u.id)
            .collect::<Vec<_>>();
        if let Terrain::Map { overlay, .. } = &mut self.terrain {
            overlay.remove_bridge(span);
        }
        for id in drowned {
            if let Some(&index) = self.lookup.get(&id) {
                self.kill_unit(index);
            }
        }
        self.collision_closed(span.footprint());
        self.collision_opened(span.footprint());
    }
}
