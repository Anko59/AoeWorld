use crate::GroundMaterial;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Biome {
    Temperate,
    Boreal,
    Tropical,
    Woodland,
    Savanna,
    Steppe,
    Desert,
    Tundra,
    Alpine,
    Polar,
}

pub(crate) fn biome_from_potential_class(class: u8) -> Option<Biome> {
    Some(match class {
        1..=3 => Biome::Tropical,
        4 | 7 => Biome::Woodland,
        8 | 9 | 13 => Biome::Temperate,
        14 | 15 => Biome::Boreal,
        16..=19 => Biome::Savanna,
        20 | 22 => Biome::Steppe,
        27 => Biome::Desert,
        28 | 30..=32 => Biome::Tundra,
        _ => return None,
    })
}

pub(crate) fn material_for(biome: Biome, height: i32) -> GroundMaterial {
    if height > 3_500 {
        return GroundMaterial::Rock;
    }
    match biome {
        Biome::Tropical => GroundMaterial::LushGrass,
        Biome::Boreal | Biome::Tundra | Biome::Polar => GroundMaterial::Snow,
        Biome::Woodland => GroundMaterial::ForestFloor,
        Biome::Savanna | Biome::Steppe => GroundMaterial::DryGrass,
        Biome::Desert => GroundMaterial::Sand,
        Biome::Alpine => GroundMaterial::Rock,
        Biome::Temperate => GroundMaterial::TemperateGrass,
    }
}

/// Initial occupied-tile tree targets for the game model, before water and
/// land-use exclusions. Terrain applies a correlated local modifier so these
/// do not form a uniform checkerboard.
pub(crate) const fn tree_density_per_thousand(biome: Biome) -> u16 {
    match biome {
        Biome::Tropical => 650,
        Biome::Temperate => 550,
        Biome::Boreal => 500,
        Biome::Woodland => 200,
        Biome::Savanna => 80,
        Biome::Steppe => 10,
        Biome::Desert | Biome::Tundra | Biome::Alpine | Biome::Polar => 0,
    }
}

pub(crate) fn tree_present(key: [u8; 32], x: i32, y: i32, biome: Biome) -> bool {
    let base = tree_density_per_thousand(biome);
    if base == 0 {
        return false;
    }
    let regional =
        75 + forest_noise(key, b"forest-density", x.div_euclid(16), y.div_euclid(16)) % 51;
    let density = u64::from(base) * regional / 100;
    forest_noise(key, b"forest-tree", x, y) % 1_000 < density
}

fn forest_noise(key: [u8; 32], domain: &[u8], x: i32, y: i32) -> u64 {
    let mut hash = blake3::Hasher::new_keyed(&key);
    hash.update(domain);
    hash.update(&x.to_le_bytes());
    hash.update(&y.to_le_bytes());
    u64::from_le_bytes(hash.finalize().as_bytes()[..8].try_into().unwrap_or([0; 8]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_classes_keep_their_published_biome_groups() {
        assert_eq!(biome_from_potential_class(1), Some(Biome::Tropical));
        assert_eq!(biome_from_potential_class(15), Some(Biome::Boreal));
        assert_eq!(biome_from_potential_class(27), Some(Biome::Desert));
        assert_eq!(biome_from_potential_class(0), None);
    }

    #[test]
    fn game_tree_targets_follow_the_biome_defaults() {
        assert_eq!(tree_density_per_thousand(Biome::Tropical), 650);
        assert_eq!(tree_density_per_thousand(Biome::Temperate), 550);
        assert_eq!(tree_density_per_thousand(Biome::Boreal), 500);
        assert_eq!(tree_density_per_thousand(Biome::Woodland), 200);
        assert_eq!(tree_density_per_thousand(Biome::Savanna), 80);
        assert_eq!(tree_density_per_thousand(Biome::Steppe), 10);
        assert_eq!(tree_density_per_thousand(Biome::Desert), 0);
    }

    #[test]
    fn forests_use_correlated_biome_specific_tree_density() {
        let key = [9; 32];
        let tropical = (0..64)
            .flat_map(|y| (0..64).map(move |x| tree_present(key, x, y, Biome::Tropical)))
            .filter(|tree| *tree)
            .count();
        let woodland = (0..64)
            .flat_map(|y| (0..64).map(move |x| tree_present(key, x, y, Biome::Woodland)))
            .filter(|tree| *tree)
            .count();
        assert!(tropical > woodland);
        assert!(!tree_present(key, 3, 3, Biome::Desert));
        assert_eq!(
            tree_present(key, 17, 24, Biome::Tropical),
            tree_present(key, 17, 24, Biome::Tropical)
        );
    }
}
