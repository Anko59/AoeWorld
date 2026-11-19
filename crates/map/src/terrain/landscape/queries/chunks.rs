//! Bounded composed scene generation: the only chunk/point composition path.
use super::*;
#[path = "chunks/surface.rs"]
mod surface;
use crate::terrain::{resource_id, resources, unsigned_noise};
use surface::{appearance, material, soft_ground, tree_family};
#[path = "chunks/dressing.rs"]
mod dressing;
#[path = "chunks/memo.rs"]
mod memo;
#[path = "chunks/starting.rs"]
mod starting;
use crate::{
    Biome, CHUNK_TILES, DecorationFamily, EcologicalPalette, GroundMaterial, LandscapeAppearance,
    LandscapeChunk, LandscapeDecoration, LandscapeResource, LandscapeTile, NativeExposure,
    NativeHeightBand, ObjectKind, ResourceKind, ResourceNode, ResourceVisualFamily, SurfaceKind,
};

impl MapChunkGenerator {
    /// Assemble an explicit candidate from source/base terrain. Policies and
    /// reservations must be stable world-coordinate functions, including across
    /// neighboring chunks. Resource approaches are included in the SAME mask
    /// before singleton filtering, never removed only from trees afterward.
    /// No gameplay, chunk cache, package or depletion overlay is mutated.
    pub fn evaluate_landscape_chunk_with_cancel(
        &self,
        x: i32,
        y: i32,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reserved_at: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<LandscapeChunk>, EnvironmentPageError> {
        self.evaluate_chunk::<false>(x, y, policy_at, reserved_at, cancelled)
    }

    /// The same chunk without decorations: identical tiles and resources for
    /// physical consumers (walk and gameplay chunks) that discard dressing.
    pub(in crate::terrain) fn evaluate_physical_chunk_with_cancel(
        &self,
        x: i32,
        y: i32,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reserved_at: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<LandscapeChunk>, EnvironmentPageError> {
        self.evaluate_chunk::<true>(x, y, policy_at, reserved_at, cancelled)
    }

    fn evaluate_chunk<const PHYSICAL: bool>(
        &self,
        x: i32,
        y: i32,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reserved_at: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<LandscapeChunk>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let Some(ox) = x.checked_mul(CHUNK_TILES) else {
            return Err(EnvironmentPageError::Invalid);
        };
        let Some(oy) = y.checked_mul(CHUNK_TILES) else {
            return Err(EnvironmentPageError::Invalid);
        };
        if !self.landscape_in_bounds(TileCoord::new(ox, oy)) {
            return Ok(None);
        }
        let mut chunk = LandscapeChunk {
            x,
            y,
            tiles: Vec::with_capacity(1024),
            resources: Vec::new(),
            decorations: Vec::new(),
        };
        let memo = memo::ChunkMemo::new_chunk(self, TileCoord::new(ox, oy), cancelled);
        for ly in 0..CHUNK_TILES {
            for lx in 0..CHUNK_TILES {
                let position = TileCoord::new(
                    ox.checked_add(lx).ok_or(EnvironmentPageError::Invalid)?,
                    oy.checked_add(ly).ok_or(EnvironmentPageError::Invalid)?,
                );
                if let Some(point) = self
                    .evaluate_landscape_point_with_memo::<false, PHYSICAL, 1296, 2500>(
                        position,
                        policy_at,
                        reserved_at,
                        cancelled,
                        &memo,
                    )?
                {
                    chunk.tiles.push(point.tile);
                    if let Some(resource) = point.resource {
                        chunk.resources.push(resource);
                    }
                    if let Some(decoration) = point.decoration {
                        chunk.decorations.push(decoration);
                    }
                }
            }
        }
        Ok(Some(chunk))
    }

    pub(in crate::terrain) fn evaluate_landscape_point_with_cancel<const PHYSICAL: bool>(
        &self,
        position: TileCoord,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reserved_at: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<super::LandscapePoint>, EnvironmentPageError> {
        let memo = memo::PointMemo::new(self, position, cancelled);
        self.evaluate_landscape_point_with_memo::<false, PHYSICAL, 25, 361>(
            position,
            policy_at,
            reserved_at,
            cancelled,
            &memo,
        )
    }

    /// The exact composed land surface without constructing resource or dressing
    /// objects. Strategic rasters consume only this projection; gameplay keeps
    /// the complete point query. Shared exclusions still determine canopy.
    pub fn landscape_surface_with_cancel(
        &self,
        position: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<super::LandscapePoint>, EnvironmentPageError> {
        let memo = memo::PointMemo::new(self, position, cancelled);
        self.evaluate_landscape_point_with_memo::<true, true, 25, 361>(
            position,
            &crate::terrain::landscape_dispatch::default_policy,
            &|at| self.landscape_reservations_at(at),
            cancelled,
            &memo,
        )
    }

    fn evaluate_landscape_point_with_memo<
        const SURFACE_ONLY: bool,
        const PHYSICAL: bool,
        const N: usize,
        const W: usize,
    >(
        &self,
        position: TileCoord,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reserved_at: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
        memo: &memo::Memo<'_, N, W>,
    ) -> Result<Option<super::LandscapePoint>, EnvironmentPageError> {
        let effective_policy = |position, base: Tile| {
            let mut policy = policy_at(position, base);
            // Explicit temperate-summer model policy, not reconstructed climate
            // evidence. Keep all source heights/corners and passability intact.
            if policy.support_per_thousand <= 1000
                && matches!(
                    base.biome,
                    Biome::Temperate | Biome::Boreal | Biome::Woodland
                )
                && base.geographic_height_centimeters >= 230_000
            {
                policy.support_per_thousand = 0;
            }
            // No forest on snow or bare rock, wherever the snow line is.
            if policy.support_per_thousand <= 1000
                && self
                    .ground_cover(position, base.geographic_height_centimeters, base.biome)
                    .is_some()
            {
                policy.support_per_thousand = 0;
            }
            policy
        };
        let shared_reservations = |position| {
            let mut reservations = reserved_at(position);
            reservations.resource_approach |= memo.reserved(position)?;
            reservations.shore |= matches!(
                memo.shore(position)?,
                memo::Shore::Beach | memo::Shore::Bank
            );
            Ok(reservations)
        };
        let composed = |at| {
            memo.landscape(at, || {
                self.evaluate_landscape_inner_with_base(
                    at,
                    &effective_policy,
                    &shared_reservations,
                    cancelled,
                    &|at| memo.base(at)?.ok_or(EnvironmentPageError::Invalid),
                    &|cx, cy| memo.wildness(cx, cy),
                )
            })
        };
        let Some((sample, base)) = composed(position)? else {
            return Ok(None);
        };
        let appearance = appearance(base, sample.density);
        let mut terrain = base;
        let cover = self.ground_cover(position, base.geographic_height_centimeters, base.biome);
        terrain.material = material(base, appearance.floor_strength, cover);
        // Shore band: appearance on land, a shallow rim in open water. It
        // never changes water kind, passability, heights or the shaped outline.
        let shore = memo.shore(position)?;
        match shore {
            memo::Shore::None => {}
            memo::Shore::Shallow | memo::Shore::Margin => terrain.material = GroundMaterial::Shore,
            memo::Shore::Deep => terrain.material = GroundMaterial::DeepWater,
            memo::Shore::Bank if base.passable => terrain.material = GroundMaterial::Dirt,
            memo::Shore::Beach if base.passable && soft_ground(terrain.material) => {
                terrain.material = GroundMaterial::Sand;
            }
            _ => {}
        }
        if base.passable && matches!(shore, memo::Shore::None) && cover.is_none() {
            // Keep climatic cover and textured banks intact. Drawn variety
            // belongs to uncovered, unbanded land and changes no physics.
            terrain.material = variety::vary(
                variety::seed(self.geography_key, self.procedural_seed),
                position,
                terrain.material,
                appearance.height_band,
                sample.historical_land_use,
            );
        }
        let rock_anchor = |at| {
            memo.rock(at, || {
                dressing::rock_anchor(self, memo, at, &composed, &shared_reservations)
            })
        };
        let mut rock = false;
        if base.passable && base.water == crate::WaterKind::None {
            rock = rock_anchor(position)?;
            terrain.passable = !rock;
            if !rock {
                for at in dressing::rock_footprint(position).filter(|at| *at != position) {
                    if rock_anchor(at)? {
                        terrain.passable = false;
                        break;
                    }
                }
            }
        }
        let tile = LandscapeTile {
            tile: position,
            terrain,
            appearance,
        };
        if SURFACE_ONLY {
            return Ok(Some(super::LandscapePoint {
                tile,
                historical_land_use: sample.historical_land_use,
                resource: None,
                decoration: None,
            }));
        }
        let open = sample.density.opening != crate::landscape_patches::Opening::None;
        let reserved = shared_reservations(position)?;
        let node = if let Some(fish) = memo.fish(position, terrain)? {
            Some(fish)
        } else if !reserved.route && !reserved.start && !open {
            memo.resource(position, base)?.filter(|node| {
                node.object != ObjectKind::ForageBush || soft_ground(terrain.material)
            })
        } else {
            None
        };
        let resource = if let Some(node) = node {
            Some(LandscapeResource {
                node,
                visual_family: match node.object {
                    ObjectKind::ShoreFish => ResourceVisualFamily::ShoreFish,
                    ObjectKind::DeepFish => ResourceVisualFamily::DeepFish,
                    _ => ResourceVisualFamily::Generic,
                },
            })
        } else if sample.density.tree {
            let visual_family = families::source_tree_family(
                self.geography_key,
                self.procedural_seed,
                self.source_biome_class_with_cancel(position, cancelled)?,
                base.biome,
                position,
            );
            let value = unsigned_noise(self.geography_key, b"objects", position.x, position.y)
                ^ self.procedural_seed.rotate_left(17);
            Some(LandscapeResource {
                node: ResourceNode {
                    id: resource_id(position, 0),
                    tile: position,
                    kind: ResourceKind::Wood,
                    object: ObjectKind::Tree,
                    initial_amount: 100,
                    visual_variant: (value >> 8) as u8,
                },
                visual_family,
            })
        } else {
            None
        };
        if PHYSICAL {
            return Ok(Some(super::LandscapePoint {
                tile,
                historical_land_use: sample.historical_land_use,
                resource,
                decoration: None,
            }));
        }
        let mut decoration = None;
        if base.surface.kind == SurfaceKind::CliffLedge
            && let Some(piece) = memo.cliff(position)?
            && piece.anchor
        {
            decoration = Some(LandscapeDecoration {
                tile: position,
                family: DecorationFamily::Cliff,
                variant: piece.variant,
                orientation: piece.facing,
            });
        }
        if resource.is_none()
            && (rock || terrain.passable)
            && base.passable
            && base.water == crate::WaterKind::None
            && dressing::uncultivated(sample.historical_land_use)
            && dressing::allows_dressing(reserved)
        {
            let roll = memo.dressing_roll(position);
            let family = if rock {
                Some(DecorationFamily::Stone)
            } else {
                dressing::family(self, position, terrain, appearance, roll)
            };
            if let Some(family) = family {
                decoration = Some(LandscapeDecoration {
                    tile: position,
                    family,
                    variant: (roll >> 8) as u8,
                    orientation: ((roll >> 16) & 7) as u8,
                });
            }
        }
        Ok(Some(super::LandscapePoint {
            tile,
            historical_land_use: sample.historical_land_use,
            resource,
            decoration,
        }))
    }

    fn landscape_base_at(
        &self,
        position: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Tile>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if !self.landscape_in_bounds(position) {
            return Ok(None);
        }
        if self.provider.is_some() {
            provider::sample_base_tile(self, position, cancelled).map(Some)
        } else {
            Ok(Some(self.sample_base_tile(position)))
        }
    }

    /// Raw source PNV class at a tile, from the same field the base biome uses.
    /// Missing vegetation fields return `None`; page failures propagate.
    pub(in crate::terrain) fn source_biome_class_with_cancel(
        &self,
        position: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<u8>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.provider.is_none() {
            return Ok(self
                .biome
                .as_ref()
                .and_then(|biome| biome.class_at(position, self.width_tiles)));
        }
        let axis = self
            .provider_environment
            .as_ref()
            .ok_or(EnvironmentPageError::Invalid)?
            .vegetation_samples_per_axis();
        match axis {
            Some(axis) => provider::sample_biome_class(self, axis, position, cancelled),
            None => Ok(None),
        }
    }

    #[cfg(test)]
    fn landscape_resource_reserved(
        &self,
        position: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(x) = position.x.checked_add(dx) else {
                    continue;
                };
                let Some(y) = position.y.checked_add(dy) else {
                    continue;
                };
                let candidate = TileCoord::new(x, y);
                if let Some(base) = self.landscape_base_at(candidate, cancelled)?
                    && resources::candidate_unreserved(self, candidate, base).is_some()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    #[cfg(test)]
    fn landscape_resource_at(
        &self,
        position: TileCoord,
        base: Tile,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        let Some(node) = resources::candidate_unreserved(self, position, base) else {
            return Ok(None);
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx == 0) == (dy == 0) {
                    continue;
                }
                let Some(x) = position.x.checked_add(dx) else {
                    continue;
                };
                let Some(y) = position.y.checked_add(dy) else {
                    continue;
                };
                let neighbor = TileCoord::new(x, y);
                if let Some(sample) = self.landscape_base_at(neighbor, cancelled)?
                    && sample.passable
                    && sample.surface.walkable()
                    && (i32::from(base.game_height_level) - i32::from(sample.game_height_level))
                        .abs()
                        <= 1
                    && resources::candidate_unreserved(self, neighbor, sample).is_none()
                {
                    return Ok(Some(node));
                }
            }
        }
        Ok(None)
    }
}

#[path = "chunks/tests.rs"]
#[cfg(test)]
mod tests;

#[path = "chunks/families.rs"]
mod families;
#[path = "chunks/variety.rs"]
mod variety;
