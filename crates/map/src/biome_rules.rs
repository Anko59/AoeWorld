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

pub(crate) fn resource_modulus(biome: Biome) -> u64 {
    match biome {
        Biome::Tropical | Biome::Temperate | Biome::Boreal => 3,
        Biome::Woodland => 6,
        Biome::Savanna => 12,
        Biome::Steppe => 96,
        Biome::Desert | Biome::Tundra | Biome::Alpine | Biome::Polar => u64::MAX,
    }
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
}
