//! One variable-length, axis-aligned bridge with land banks and a water deck.
use crate::{
    BuildingKind, EconomyError, GameBuilding, GameWorld, GameWorldError, Stockpile, Terrain,
};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::{GroundMaterial, WaterKind};

impl GameBuilding {
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
    /// Place one bridge between two land banks. One to forty intervening water
    /// tiles; cardinal axes only. Validate the entire order before charging.
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
        let rect = Self::bridge_footprint(from, to).ok_or(EconomyError::InvalidFootprint)?;
        let count = rect.width().max(rect.height()) as u32 - 2;
        let cost = self.check_buildable(player, BuildingKind::Bridge, count)?;
        self.validate_explored_footprint(player, rect)?;
        self.validate_bridge_placement(rect)?;
        let id = self.insert_building(player, BuildingKind::Bridge, rect, false)?;
        self.charge(player, cost);
        if !self.assign_build_orders(&builders, &[id], queued) {
            let _ = self.issue_cancel_construction(player, id);
            return Err(GameWorldError::Unreachable.into());
        }
        self.remember_visible_buildings();
        Ok(id)
    }

    pub fn bridge_footprint(from: TileCoord, to: TileCoord) -> Option<TileRect> {
        let length = from.x.abs_diff(to.x).max(from.y.abs_diff(to.y));
        if (from.x != to.x && from.y != to.y) || !(2..=41).contains(&length) {
            return None;
        }
        Some(TileRect::new(
            TileCoord::new(from.x.min(to.x), from.y.min(to.y)),
            TileCoord::new(
                from.x.max(to.x).checked_add(1)?,
                from.y.max(to.y).checked_add(1)?,
            ),
        ))
    }

    fn validate_bridge_placement(&self, rect: TileRect) -> Result<(), EconomyError> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Err(EconomyError::InvalidFootprint);
        };
        if !rect.valid(self.config.width_tiles, self.config.height_tiles)
            || self.query(rect).0.iter().any(|u| u.hp > 0)
            || self
                .buildings()
                .any(|b| super::placement::rects_overlap(b.footprint, rect))
        {
            return Err(EconomyError::InvalidFootprint);
        }
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let tile = TileCoord::new(x, y);
                let end =
                    tile == rect.min || tile == TileCoord::new(rect.max.x - 1, rect.max.y - 1);
                let (sample, node) = generator
                    .tile_and_node_with_cancel(tile, &|| false)
                    .map_err(GameWorldError::Environment)?
                    .ok_or(EconomyError::InvalidFootprint)?;
                if overlay.blocks_tile(tile)
                    || overlay.bridge_axis(tile).is_some()
                    || node.is_some_and(|n| overlay.blocks_node(n))
                    || if end {
                        !sample.passable || sample.water != WaterKind::None
                    } else {
                        sample.water == WaterKind::None || sample.material == GroundMaterial::Ford
                    }
                {
                    return Err(EconomyError::InvalidFootprint);
                }
            }
        }
        Ok(())
    }

    pub(super) fn complete_bridge(&mut self, rect: TileRect) -> bool {
        let Terrain::Map { overlay, .. } = &mut self.terrain else {
            return false;
        };
        if !overlay.complete_bridge(rect) {
            return false;
        }
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                self.economy.blocked_tiles.remove(&TileCoord::new(x, y));
            }
        }
        self.collision_opened(rect);
        true
    }

    pub(super) fn remove_bridge_deck(&mut self, rect: TileRect) {
        // Land units on the collapsed water deck die deterministically. Banks
        // remain safe; ships have never been allowed onto this deck.
        let drowned: Vec<_> = self
            .query(rect)
            .0
            .into_iter()
            .filter(|u| {
                u.hp > 0
                    && u.position.tile_floor() != rect.min
                    && u.position.tile_floor() != TileCoord::new(rect.max.x - 1, rect.max.y - 1)
            })
            .map(|u| u.id)
            .collect();
        if let Terrain::Map { overlay, .. } = &mut self.terrain {
            overlay.remove_bridge(rect);
        }
        for id in drowned {
            if let Some(&index) = self.lookup.get(&id) {
                self.kill_unit(index);
            }
        }
        self.collision_closed(rect);
        // The same change opens the naval domain; discard local sealed proofs.
        self.collision_opened(rect);
    }
}
