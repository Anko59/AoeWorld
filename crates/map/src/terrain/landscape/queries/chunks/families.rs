//! Source-backed tree visual families.
//!
//! Applied only to trees already selected by the coherent density descriptor.
//! It does not alter density, terrain, resources, reservations, IDs,
//! positions, amounts or variants.
use super::{Biome, ResourceVisualFamily, TileCoord, tree_family, unsigned_noise};

/// Modeled mixed-stand edge, in world tiles; not a measured canopy size.
const MIXED_STAND_TILES: i32 = 16;

/// Locked PNV v0.2 classes retain distinctions lost by the coarse biome.
/// Legend: Zenodo record 3526620, pnv_biome.type_biome00k_c_250m_s0..0cm_
/// 2000..2017_v0.2.tif.csv, SHA256
/// bd95b5d365c2b1abc2a4797e365e61a6fe3f7814a64596cc2ee734cc35fc1eae.
/// Classes 8/15/17 are explicitly needleleaf and 13 explicitly broadleaf.
/// Class 14 "cold deciduous forest" does not prove broadleaf (e.g. larch), so
/// it keeps the biome fallback. Class 9 uses modeled equally likely 16×16
/// WORLD-TILE stands, NOT measured physical canopy size, observed botanical
/// proportion, climate reconstruction, or evidence about any map. Euclidean
/// blocks preserve spatial correlation across signed coordinates. Other or
/// missing classes use the biome-family fallback `tree_family`, including the
/// stylized Tropical presentation; this approves no new assets.
pub(super) fn source_tree_family(
    mut geography_key: [u8; 32],
    procedural_seed: u64,
    source_class: Option<u8>,
    biome: Biome,
    position: TileCoord,
) -> ResourceVisualFamily {
    match source_class {
        Some(8 | 15 | 17) => ResourceVisualFamily::Conifer,
        Some(13) => ResourceVisualFamily::Broadleaf,
        Some(9) => {
            for (key_byte, seed_byte) in geography_key.iter_mut().zip(procedural_seed.to_le_bytes())
            {
                *key_byte ^= seed_byte;
            }
            let value = unsigned_noise(
                geography_key,
                b"pnv-mixed-stand",
                position.x.div_euclid(MIXED_STAND_TILES),
                position.y.div_euclid(MIXED_STAND_TILES),
            );
            if value & 1 == 0 {
                ResourceVisualFamily::Broadleaf
            } else {
                ResourceVisualFamily::Conifer
            }
        }
        _ => tree_family(biome),
    }
}

#[path = "families/tests.rs"]
#[cfg(test)]
mod tests;
