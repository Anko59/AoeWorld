//! Dense base terrain: bound vector fields where present and the seeded
//! no-source fallback relief, water and biome elsewhere. The composed
//! landscape decorates this base; it is never a separate generation path.
use super::{
    Biome, GroundMaterial, MapChunkGenerator, Provenance, Tile, WaterKind, compression_fallback,
    quantize_game_height, signed_noise, surface, unsigned_noise,
};
use crate::biome_rules::{biome_from_potential_class, material_for};
use aoe_core::TileCoord;

impl MapChunkGenerator {
    /// Undecorated terrain for neighbor queries; never samples landscape or
    /// objects. Fallback noise is evaluated only for fields without a source.
    pub(super) fn sample_base_tile(&self, tile: TileCoord) -> Tile {
        let (biome, vegetation_provenance) = self
            .biome
            .as_ref()
            .and_then(|biome| biome.class_at(tile, self.width_tiles))
            .and_then(biome_from_potential_class)
            .map(|biome| (biome, Provenance::SourceDerived))
            .unwrap_or_else(|| (fallback_biome(self, tile), Provenance::Fallback));
        let elevation = self.elevation.as_ref().and_then(|elevation| {
            let height = elevation.height_at(tile, self.width_tiles)?;
            let corner_heights = elevation
                .corner_heights(tile, self.width_tiles)
                .unwrap_or([height; 4]);
            Some((height, corner_heights, elevation.compression))
        });
        let (geographic_height_centimeters, game_height_level, surface, elevation_provenance) =
            match elevation {
                Some((height, corner_heights, compression)) => (
                    height,
                    quantize_game_height(height, compression),
                    surface::from_heights(corner_heights, compression),
                    Provenance::SourceDerived,
                ),
                None => {
                    let height = fallback_height(self, tile);
                    (
                        height,
                        quantize_game_height(height, compression_fallback()),
                        surface::from_heights([height; 4], compression_fallback()),
                        Provenance::Fallback,
                    )
                }
            };
        // Elevation cannot identify water: inland depressions can be dry, while
        // coastlines require independent, coherent water geometry.
        let (water, water_provenance) = self
            .water
            .as_ref()
            .and_then(|water| water.coverage_at(tile, self.width_tiles))
            .map(|coverage| water_from_coverage(coverage.ocean_percent, coverage.inland_percent))
            .unwrap_or_else(|| (fallback_water(self, tile), Provenance::Fallback));
        let material = match water {
            WaterKind::None => material_for(biome, geographic_height_centimeters),
            WaterKind::River | WaterKind::Lake | WaterKind::Ocean => GroundMaterial::Water,
            WaterKind::Shallow => GroundMaterial::Shore,
        };
        Tile {
            geographic_height_centimeters,
            game_height_level,
            surface,
            material,
            biome,
            vegetation_provenance,
            water,
            elevation_provenance,
            water_provenance,
            hydrology_observation: None,
            modern_land_cover_class: None,
            passable: water == WaterKind::None
                && material != GroundMaterial::Ice
                && surface.walkable(),
        }
    }
}

/// Source water coverage percentages to a water kind.
pub(super) fn water_from_coverage(ocean: u8, inland: u8) -> (WaterKind, Provenance) {
    match ocean {
        1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
        51..=100 => (WaterKind::Ocean, Provenance::SourceDerived),
        _ => match inland {
            0 => (WaterKind::None, Provenance::SourceDerived),
            1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
            _ => (WaterKind::Lake, Provenance::SourceDerived),
        },
    }
}

pub(super) fn fallback_height(generator: &MapChunkGenerator, tile: TileCoord) -> i32 {
    signed_noise(
        generator.geography_key,
        b"relief",
        tile.x.div_euclid(8),
        tile.y.div_euclid(8),
    )
    .saturating_mul(25)
    .saturating_add(signed_noise(generator.geography_key, b"relief-detail", tile.x, tile.y) / 8)
}

pub(super) fn fallback_water(generator: &MapChunkGenerator, tile: TileCoord) -> WaterKind {
    if unsigned_noise(
        generator.geography_key,
        b"water",
        tile.x.div_euclid(16),
        tile.y.div_euclid(16),
    )
    .is_multiple_of(97)
    {
        WaterKind::Lake
    } else if unsigned_noise(
        generator.geography_key,
        b"river",
        tile.x.div_euclid(4),
        tile.y.div_euclid(4),
    )
    .is_multiple_of(521)
    {
        WaterKind::River
    } else {
        WaterKind::None
    }
}

pub(super) fn fallback_biome(generator: &MapChunkGenerator, tile: TileCoord) -> Biome {
    match unsigned_noise(
        generator.geography_key,
        b"biome",
        tile.x.div_euclid(32),
        tile.y.div_euclid(32),
    ) % 10
    {
        0 => Biome::Tropical,
        1 => Biome::Boreal,
        2 => Biome::Woodland,
        3 => Biome::Savanna,
        4 => Biome::Steppe,
        5 => Biome::Desert,
        6 => Biome::Tundra,
        7 => Biome::Alpine,
        8 => Biome::Polar,
        _ => Biome::Temperate,
    }
}
