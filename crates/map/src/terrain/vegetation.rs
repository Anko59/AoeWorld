//! Potential-vegetation biome of a tile, shared by dense and provider base
//! terrain. A source sample without a usable class (nodata, typically beside
//! lakes and coasts) takes the nearest usable class within 32 samples instead
//! of the seeded no-source biome noise, whose 32-tile cells would otherwise
//! appear as chunk-aligned squares of unrelated biomes inside real vegetation.
use super::fallback::fallback_biome;
use super::{Biome, MapChunkGenerator, Provenance};
use crate::biome_rules::biome_from_potential_class;
use aoe_core::TileCoord;

const DIRECTIONS: [(i64, i64); 8] = [
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
    (1, -1),
];
const RINGS: [i64; 6] = [1, 2, 4, 8, 16, 32];

/// At most 49 class lookups. A filled tile keeps `Provenance::Fallback`: its
/// biome is a neighbor's class, not an observation at that sample.
pub(super) fn filled_biome<E>(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    samples_per_axis: u16,
    class_at: impl Fn(TileCoord) -> Result<Option<u8>, E>,
) -> Result<(Biome, Provenance), E> {
    if let Some(biome) = class_at(tile)?.and_then(biome_from_potential_class) {
        return Ok((biome, Provenance::SourceDerived));
    }
    // Source samples sit `(width - 1) / (axis - 1)` tiles apart. Offsets are
    // clamped into the map, so a border tile still finds its inland class.
    let last = i64::from(generator.width_tiles.max(1)) - 1;
    let step = (last / (i64::from(samples_per_axis) - 1).max(1)).max(1) + 1;
    for ring in RINGS {
        for (dx, dy) in DIRECTIONS {
            let x = (i64::from(tile.x) + dx * ring * step).clamp(0, last);
            let y = (i64::from(tile.y) + dy * ring * step).clamp(0, last);
            let neighbor = TileCoord::new(x as i32, y as i32);
            if let Some(biome) = class_at(neighbor)?.and_then(biome_from_potential_class) {
                return Ok((biome, Provenance::Fallback));
            }
        }
    }
    Ok((fallback_biome(generator, tile), Provenance::Fallback))
}
