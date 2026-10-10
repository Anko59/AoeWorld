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
    if height > 350_000 {
        return GroundMaterial::Rock;
    }
    match biome {
        Biome::Tropical => GroundMaterial::LushGrass,
        Biome::Boreal | Biome::Tundra => GroundMaterial::Snow,
        Biome::Polar => GroundMaterial::Ice,
        Biome::Woodland => GroundMaterial::ForestFloor,
        Biome::Savanna | Biome::Steppe => GroundMaterial::DryGrass,
        Biome::Desert => GroundMaterial::Sand,
        Biome::Alpine => GroundMaterial::Rock,
        Biome::Temperate => GroundMaterial::TemperateGrass,
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

    #[test]
    fn polar_biomes_are_ice_and_cannot_supply_a_land_start() {
        assert_eq!(material_for(Biome::Tundra, 0), GroundMaterial::Snow);
        assert_eq!(material_for(Biome::Polar, 0), GroundMaterial::Ice);
    }

    #[test]
    fn material_height_threshold_uses_centimeters() {
        assert_eq!(
            material_for(Biome::Temperate, 350_000),
            GroundMaterial::TemperateGrass
        );
        assert_eq!(
            material_for(Biome::Temperate, 350_001),
            GroundMaterial::Rock
        );
    }
}
