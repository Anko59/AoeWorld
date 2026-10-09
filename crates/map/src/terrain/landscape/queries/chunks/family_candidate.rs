//! UNWIRED source-aware visual-family candidate, compiled only for pure tests.
//!
//! A future separately identified recipe must preserve readable recipe 9 and
//! legacy defaults before using this policy on already-eligible density trees.
//! This module does not sample sources or alter density, terrain, resources,
//! reservations, caches, package identity, or renderer/artwork approval.
use super::{Biome, ResourceVisualFamily, TileCoord, tree_family, unsigned_noise};

/// Locked PNV v0.2 classes retain distinctions lost by the coarse biome.
/// Legend: Zenodo record 3526620, pnv_biome.type_biome00k_c_250m_s0..0cm_
/// 2000..2017_v0.2.tif.csv, SHA256
/// bd95b5d365c2b1abc2a4797e365e61a6fe3f7814a64596cc2ee734cc35fc1eae.
/// Class 9 uses modeled equally likely 16×16 WORLD-TILE stands, NOT measured
/// physical canopy size, observed botanical proportion, climate reconstruction,
/// or evidence about any map. Their physical size depends on future tile scale.
/// Euclidean blocks preserve spatial correlation across signed coordinates.
/// Other/missing classes retain the existing biome-family fallback, including
/// the existing stylized Tropical presentation; this approves no new assets.
#[cfg(test)]
pub(crate) fn candidate_source_tree_family(
    mut geography_key: [u8; 32],
    procedural_seed: u64,
    source_class: Option<u8>,
    biome: Biome,
    position: TileCoord,
) -> ResourceVisualFamily {
    match source_class {
        Some(8 | 15 | 17) => ResourceVisualFamily::Conifer,
        Some(13 | 14) => ResourceVisualFamily::Broadleaf,
        Some(9) => {
            for (key_byte, seed_byte) in geography_key.iter_mut().zip(procedural_seed.to_le_bytes())
            {
                *key_byte ^= seed_byte;
            }
            let value = unsigned_noise(
                geography_key,
                b"candidate-pnv-mixed-stand",
                position.x.div_euclid(16),
                position.y.div_euclid(16),
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

#[path = "family_candidate/tests.rs"]
#[cfg(test)]
mod tests;
