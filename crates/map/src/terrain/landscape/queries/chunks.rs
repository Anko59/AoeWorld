//! Bounded composed scene generation: the only chunk/point composition path.
use super::*;
use crate::terrain::{resource_id, resources, unsigned_noise};
#[path = "chunks/memo.rs"]
mod memo;
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
                if let Some(point) = self.evaluate_landscape_point_with_memo(
                    position,
                    policy_at,
                    reserved_at,
                    cancelled,
                    &memo,
                )? {
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

    pub(in crate::terrain) fn evaluate_landscape_point_with_cancel(
        &self,
        position: TileCoord,
        policy_at: &dyn Fn(TileCoord, Tile) -> LandscapePolicy,
        reserved_at: &dyn Fn(TileCoord) -> Reservations,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<super::LandscapePoint>, EnvironmentPageError> {
        let memo = memo::PointMemo::new(self, position, cancelled);
        self.evaluate_landscape_point_with_memo(position, policy_at, reserved_at, cancelled, &memo)
    }

    fn evaluate_landscape_point_with_memo<const N: usize, const W: usize>(
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
        let Some((sample, base)) = self.evaluate_landscape_inner_with_base(
            position,
            &effective_policy,
            &shared_reservations,
            cancelled,
            &|position| memo.base(position)?.ok_or(EnvironmentPageError::Invalid),
            &|cx, cy| memo.wildness(cx, cy),
        )?
        else {
            return Ok(None);
        };
        let appearance = appearance(base, sample.density);
        let mut terrain = base;
        let cover = self.ground_cover(position, base.geographic_height_centimeters, base.biome);
        terrain.material = material(base, appearance, cover);
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
        let tile = LandscapeTile {
            tile: position,
            terrain,
            appearance,
        };
        let reserved = shared_reservations(position)?;
        // Large-forest glades and trails stay free of blocking resources.
        let open = sample.density.opening != crate::landscape_patches::Opening::None;
        let node = if !reserved.route && !reserved.start && !open {
            // Resource candidates use the source/base biome. Forage also
            // needs the final ground after snow-line and shore composition.
            memo.resource(position, base)?.filter(|node| {
                node.object != ObjectKind::ForageBush || soft_ground(terrain.material)
            })
        } else {
            None
        };
        let resource = if let Some(node) = node {
            Some(LandscapeResource {
                node,
                visual_family: ResourceVisualFamily::Generic,
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
            && base.passable
            && base.water == crate::WaterKind::None
            && !matches!(
                sample.historical_land_use,
                LandUse::Crop | LandUse::Grazing | LandUse::Nonland
            )
            && !reserved.route
            && !reserved.start
            && !reserved.resource_approach
            && !reserved.shore
        {
            let roll = unsigned_noise(
                self.geography_key,
                b"landscape-dressing-v9",
                position.x,
                position.y,
            ) ^ self.procedural_seed.rotate_left(29);
            // Loose rocks: bare rock, scree beside cliffs, a few elsewhere.
            let rock_roll = (roll >> 24) % 1000;
            if base.water == crate::WaterKind::None
                && rock_roll < crate::terrain::relief::MAX_ROCK_PER_THOUSAND
                && sample.density.canopy_per_thousand == 0
            {
                let mut beside_cliff = false;
                for (dx, dy) in crate::terrain::relief::SCREE_REACH {
                    let near = TileCoord::new(
                        position.x.saturating_add(dx),
                        position.y.saturating_add(dy),
                    );
                    if memo.base(near)?.is_some_and(|tile| {
                        tile.surface.is_cliff()
                            && tile.geographic_height_centimeters
                                > base.geographic_height_centimeters
                    }) {
                        beside_cliff = true;
                        break;
                    }
                }
                if rock_roll
                    < self.rock_per_thousand(position, terrain.material, base.biome, beside_cliff)
                {
                    decoration = Some(LandscapeDecoration {
                        tile: position,
                        family: DecorationFamily::Stone,
                        variant: (roll >> 8) as u8,
                        orientation: ((roll >> 16) & 7) as u8,
                    });
                }
            }
            // Exposed ground carries rocks only, by the rule above.
            let sheltered = sample.density.canopy_per_thousand > 0;
            if decoration.is_none()
                && roll % 100 < 3
                && (sheltered || appearance.exposure != NativeExposure::Exposed)
            {
                let family = if sheltered {
                    DecorationFamily::Deadwood
                } else if matches!(base.biome, Biome::Woodland | Biome::Savanna) {
                    DecorationFamily::Shrub
                } else {
                    DecorationFamily::Grass
                };
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

fn tree_family(biome: Biome) -> ResourceVisualFamily {
    match biome {
        Biome::Boreal => ResourceVisualFamily::Conifer,
        Biome::Tropical => ResourceVisualFamily::Tropical,
        Biome::Woodland | Biome::Savanna => ResourceVisualFamily::DryScrub,
        _ => ResourceVisualFamily::Broadleaf,
    }
}

fn appearance(base: Tile, density: DensitySample) -> LandscapeAppearance {
    let height_band = match base.geographic_height_centimeters {
        ..100_000 => NativeHeightBand::Lowland,
        100_000..180_000 => NativeHeightBand::Montane,
        180_000..250_000 => NativeHeightBand::Subalpine,
        250_000..350_000 => NativeHeightBand::Alpine,
        _ => NativeHeightBand::Nival,
    };
    let palette = match base.biome {
        Biome::Temperate => EcologicalPalette::Temperate,
        Biome::Boreal => EcologicalPalette::Boreal,
        Biome::Tropical => EcologicalPalette::Tropical,
        Biome::Woodland => EcologicalPalette::DryScrub,
        Biome::Savanna => EcologicalPalette::Savanna,
        _ => EcologicalPalette::Treeless,
    };
    let exposure = if base.surface.is_cliff()
        || base.material == GroundMaterial::Rock
        || matches!(
            height_band,
            NativeHeightBand::Alpine | NativeHeightBand::Nival
        ) {
        NativeExposure::Exposed
    } else if density.canopy_per_thousand > 0 {
        NativeExposure::Sheltered
    } else {
        NativeExposure::Open
    };
    LandscapeAppearance {
        canopy_strength: density.canopy_per_thousand,
        floor_strength: density.forest_floor_per_thousand,
        palette,
        exposure,
        height_band,
    }
}

/// Rock, snow and ice shores stay as they are; everything else takes the band.
const fn soft_ground(material: GroundMaterial) -> bool {
    !matches!(
        material,
        GroundMaterial::Rock | GroundMaterial::Snow | GroundMaterial::Ice
    )
}

/// Snow lies on everything walkable above the snow line, forest floor under
/// canopy, bare rock in the band below the snow. Cold biomes below the snow
/// line are summer grass and heath, not snow.
fn material(
    base: Tile,
    appearance: LandscapeAppearance,
    cover: Option<crate::terrain::relief::Cover>,
) -> GroundMaterial {
    use crate::terrain::relief::Cover;
    if base.water != crate::WaterKind::None || base.material == GroundMaterial::Ice {
        return base.material;
    }
    if cover == Some(Cover::Snow) {
        return GroundMaterial::Snow;
    }
    if appearance.floor_strength > 0 {
        return GroundMaterial::ForestFloor;
    }
    if cover == Some(Cover::Rock) {
        return GroundMaterial::Rock;
    }
    match base.material {
        GroundMaterial::ForestFloor => GroundMaterial::DryGrass,
        GroundMaterial::Snow if base.biome == Biome::Boreal => GroundMaterial::TemperateGrass,
        GroundMaterial::Snow => GroundMaterial::DryGrass,
        material => material,
    }
}

#[path = "chunks/tests.rs"]
#[cfg(test)]
mod tests;

#[path = "chunks/families.rs"]
mod families;
#[path = "chunks/variety.rs"]
mod variety;
