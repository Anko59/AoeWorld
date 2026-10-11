//! Dense base terrain: bound vector fields where present and the seeded
//! no-source fallback relief, water and biome elsewhere. The composed
//! landscape decorates this base; it is never a separate generation path.
use super::landscape::mix64;
use super::shoreline::{self, CellGrid};
use super::{
    Biome, GroundMaterial, MapChunkGenerator, Provenance, Tile, WaterKind, compression_fallback,
    quantize_game_height, signed_noise, surface, unsigned_noise,
};
use crate::biome_rules::material_for;
use aoe_core::TileCoord;
use std::convert::Infallible;

const LAKE_BLOCKS: CellGrid = CellGrid::blocks(16);
const RIVER_BLOCKS: CellGrid = CellGrid::blocks(4);

impl MapChunkGenerator {
    /// Geography-only key of the shoreline warp; the procedural seed never
    /// moves water.
    pub(super) fn shoreline_key(&self) -> u64 {
        self.geography_key
            .chunks_exact(8)
            .fold(0x73_686f_7265_6c69, |key, part| {
                mix64(key ^ u64::from_le_bytes(part.try_into().unwrap_or([0; 8])))
            })
    }

    /// Shaped water of the dense (pageless) fields or the seeded fallback.
    pub(super) fn dense_water(&self, tile: TileCoord) -> (WaterKind, Provenance) {
        let Some(water) = &self.water else {
            return (fallback_water(self, tile), Provenance::Fallback);
        };
        let grid = CellGrid::source(water.samples_per_axis(), self.width_tiles);
        shoreline::sample(self.shoreline_key(), grid, tile, &mut |x, y| {
            Ok::<_, Infallible>(water.cell(x, y).and_then(|coverage| {
                let water = water_from_coverage(coverage.ocean_percent, coverage.inland_percent);
                (water.0 != WaterKind::None).then_some(water)
            }))
        })
        .unwrap_or_else(|never| match never {})
        .unwrap_or((WaterKind::None, Provenance::SourceDerived))
    }

    /// Conservative: `false` proves no dense water within `radius` tiles.
    pub(super) fn dense_water_near(&self, tile: TileCoord, radius: i32) -> bool {
        let Some(water) = &self.water else {
            return fallback_water_near(self, tile, radius);
        };
        let grid = CellGrid::source(water.samples_per_axis(), self.width_tiles);
        shoreline::any_wet_near(grid, tile, radius, &mut |x, y| {
            Ok::<_, Infallible>(water.cell(x, y).is_some_and(|coverage| {
                coverage.ocean_percent != 0 || coverage.inland_percent != 0
            }))
        })
        .unwrap_or_else(|never| match never {})
    }

    /// Conservative: `true` proves only lake or ocean water within `radius`
    /// tiles. Seeded fallback water is always scanned instead.
    pub(super) fn dense_open_water_near(&self, tile: TileCoord, radius: i32) -> bool {
        let Some(water) = &self.water else {
            return false;
        };
        let grid = CellGrid::source(water.samples_per_axis(), self.width_tiles);
        shoreline::all_wet_near(grid, tile, radius, &mut |x, y| {
            Ok::<_, Infallible>(water.cell(x, y).is_some_and(|coverage| {
                open_coverage(coverage.ocean_percent, coverage.inland_percent)
            }))
        })
        .unwrap_or_else(|never| match never {})
    }

    /// Undecorated terrain for neighbor queries; never samples landscape or
    /// objects. Fallback noise is evaluated only for fields without a source.
    pub(super) fn sample_base_tile(&self, tile: TileCoord) -> Tile {
        let (biome, vegetation_provenance) = self.sample_biome_with_provenance(tile);
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
        let (water, water_provenance) = self.dense_water(tile);
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

impl MapChunkGenerator {
    /// Dense base biome with nodata samples filled from the nearest usable class.
    fn sample_biome_with_provenance(&self, tile: TileCoord) -> (Biome, Provenance) {
        let Some(biome) = self.biome.as_ref() else {
            return (fallback_biome(self, tile), Provenance::Fallback);
        };
        let filled = super::vegetation::filled_biome(self, tile, biome.samples_per_axis(), |at| {
            Ok::<_, std::convert::Infallible>(biome.class_at(at, self.width_tiles))
        });
        match filled {
            Ok(biome) => biome,
            Err(never) => match never {},
        }
    }

    pub(super) fn sample_biome(&self, tile: TileCoord) -> Biome {
        self.sample_biome_with_provenance(tile).0
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

/// Coverage that `water_from_coverage` reads as lake or ocean.
pub(super) fn open_coverage(ocean: u8, inland: u8) -> bool {
    ocean > 50 || (ocean == 0 && inland > 50)
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

fn seeded_lake(generator: &MapChunkGenerator, x: i64, y: i64) -> bool {
    unsigned_noise(generator.geography_key, b"water", x as i32, y as i32).is_multiple_of(97)
}

fn seeded_river(generator: &MapChunkGenerator, x: i64, y: i64) -> bool {
    unsigned_noise(generator.geography_key, b"river", x as i32, y as i32).is_multiple_of(521)
}

/// Seeded 16-tile lake and 4-tile river blocks, shaped like source cells.
pub(super) fn fallback_water(generator: &MapChunkGenerator, tile: TileCoord) -> WaterKind {
    let key = generator.shoreline_key();
    let wet = |blocks, seeded: fn(&MapChunkGenerator, i64, i64) -> bool| {
        shoreline::sample(key, blocks, tile, &mut |x, y| {
            Ok::<_, Infallible>(seeded(generator, x, y).then_some(()))
        })
        .unwrap_or_else(|never| match never {})
        .is_some()
    };
    if wet(LAKE_BLOCKS, seeded_lake) {
        WaterKind::Lake
    } else if wet(RIVER_BLOCKS, seeded_river) {
        WaterKind::River
    } else {
        WaterKind::None
    }
}

/// Conservative: `false` proves no seeded water within `radius` tiles.
pub(super) fn fallback_water_near(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    radius: i32,
) -> bool {
    let near = |blocks, seeded: fn(&MapChunkGenerator, i64, i64) -> bool| {
        shoreline::any_wet_near(blocks, tile, radius, &mut |x, y| {
            Ok::<_, Infallible>(seeded(generator, x, y))
        })
        .unwrap_or_else(|never| match never {})
    };
    near(LAKE_BLOCKS, seeded_lake) || near(RIVER_BLOCKS, seeded_river)
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
