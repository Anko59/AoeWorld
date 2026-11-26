//! One deterministic preparation allowance shared by every planner in a tick.
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageError, MapChunkGenerator, ResourceOverlay, RouteTerrain};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct LimitedMap<'a> {
    pub(super) generator: &'a MapChunkGenerator,
    pub(super) allowance: &'a Mutex<u32>,
    /// Set when a chunk preparation was refused for want of allowance.
    pub(super) denied: Option<&'a AtomicBool>,
    pub(super) world: &'a crate::GameWorld,
    pub(super) player: aoe_core::PlayerId,
}
impl RouteTerrain for LimitedMap<'_> {
    fn route_generator(&self) -> Option<&MapChunkGenerator> {
        Some(self.generator)
    }
    fn prepare_navigation_chunk(&self) -> bool {
        let mut left = self
            .allowance
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *left == 0 {
            if let Some(denied) = self.denied {
                denied.store(true, Ordering::Relaxed);
            }
            false
        } else {
            *left -= 1;
            true
        }
    }
    fn route_neighbor_known(
        &self,
        overlay: &ResourceOverlay,
        from: TileCoord,
        offset: (i32, i32),
    ) -> Option<bool> {
        match &self.world.terrain {
            crate::Terrain::Map { overlay: live, .. } if std::ptr::eq(overlay, live) => {
                self.world.known_route_step(self.generator, from, offset)
            }
            _ => None,
        }
    }
    fn route_tile_known(&self, overlay: &ResourceOverlay, tile: TileCoord) -> Option<bool> {
        match &self.world.terrain {
            crate::Terrain::Map { overlay: live, .. } if std::ptr::eq(overlay, live) => self
                .world
                .known_route_mask(self.generator, tile)
                .map(|mask| mask & aoe_map::STEP_MASK_WALKABLE != 0),
            _ => None,
        }
    }
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if !self.world.gate_tile_admitted(self.player, tile) {
            return Ok(false);
        }
        // On the live overlay this is movement's land passability, which the
        // world keeps per tile across ticks; a proof snapshot reads directly.
        match &self.world.terrain {
            crate::Terrain::Map { overlay: live, .. } if std::ptr::eq(overlay, live) => self
                .world
                .unit_tile_passable(crate::UnitKind::Villager, tile, cancelled),
            _ => self.generator.route_walkable(overlay, tile, cancelled),
        }
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok(self.world.gate_tile_admitted(self.player, from)
            && self.world.gate_tile_admitted(self.player, to)
            && self.generator.route_edge_clear(from, to, cancelled)?)
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        self.generator.route_step_cost(from, to, cancelled)
    }
}

/// The live ground with no gate: what every player's search reads away from
/// gates, so one step mask serves them all.
struct GroundOnly<'a> {
    generator: &'a MapChunkGenerator,
    world: &'a crate::GameWorld,
}
impl RouteTerrain for GroundOnly<'_> {
    fn route_walkable(
        &self,
        _: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.world
            .unit_tile_passable(crate::UnitKind::Villager, tile, cancelled)
    }
    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        self.generator.route_edge_clear(from, to, cancelled)
    }
    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        self.generator.route_step_cost(from, to, cancelled)
    }
}

impl crate::GameWorld {
    /// The live overlay's step from `from` by `offset`, from the kept step
    /// masks; a tile near a gate (whose admission is per player) or whose
    /// ground cannot be read answers `None`, and the caller reads the ground.
    pub(super) fn known_route_step(
        &self,
        generator: &MapChunkGenerator,
        from: TileCoord,
        offset: (i32, i32),
    ) -> Option<bool> {
        let index = aoe_map::step_index(offset)?;
        Some(self.known_route_mask(generator, from)? & (1 << index) != 0)
    }

    /// `from`'s kept mask: its eight steps and its own ground.
    pub(super) fn known_route_mask(
        &self,
        generator: &MapChunkGenerator,
        from: TileCoord,
    ) -> Option<u16> {
        match self.economy.step_masks.mask(from) {
            aoe_map::StepMask::Known(mask) => Some(mask),
            aoe_map::StepMask::Slow => None,
            aoe_map::StepMask::Unknown => {
                let mask = self.step_mask(generator, from);
                let config = self.config;
                self.economy
                    .step_masks
                    .store(config.width_tiles, config.height_tiles, from, mask);
                mask
            }
        }
    }

    fn step_mask(&self, generator: &MapChunkGenerator, from: TileCoord) -> Option<u16> {
        let crate::Terrain::Map { overlay, .. } = &self.terrain else {
            return None;
        };
        // Every tile a step from `from` reads lies within one tile of it.
        let gate_near = !self.economy.gate_tiles.is_empty()
            && (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    self.economy
                        .gate_tiles
                        .contains_key(&TileCoord::new(from.x + dx, from.y + dy))
                })
            });
        if gate_near {
            return None;
        }
        let ground = GroundOnly {
            generator,
            world: self,
        };
        let walkable = ground.route_walkable(overlay, from, &|| false).ok()?;
        let mut mask = if walkable {
            aoe_map::STEP_MASK_WALKABLE
        } else {
            0
        };
        for index in 0..8u8 {
            let offset = [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ][usize::from(index)];
            if aoe_map::route_neighbor_clear(&ground, overlay, from, offset, &|| false).ok()? {
                mask |= 1 << index;
            }
        }
        Some(mask)
    }
}
