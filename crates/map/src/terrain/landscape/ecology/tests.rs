use super::*;
use crate::landscape_patches::{Parameters, Patches, Zone};
use crate::{Provenance, SurfaceDiagonal, SurfaceKind, TileSurface};

fn base() -> Tile {
    Tile {
        geographic_height_centimeters: 100,
        game_height_level: 0,
        surface: TileSurface {
            corner_game_height_levels: [0; 4],
            kind: SurfaceKind::Plateau,
            triangulation: SurfaceDiagonal::NorthwestSoutheast,
        },
        material: GroundMaterial::TemperateGrass,
        biome: Biome::Temperate,
        vegetation_provenance: Provenance::SourceDerived,
        water: WaterKind::None,
        elevation_provenance: Provenance::SourceDerived,
        water_provenance: Provenance::SourceDerived,
        hydrology_observation: None,
        modern_land_cover_class: None,
        passable: true,
    }
}

fn input(tile: Tile, history: LandUse, reservations: Reservations) -> Input {
    assess(tile, Region::Heavy, 1000, history, reservations)
        .expect("valid policy")
        .input
}

#[test]
fn historical_observation_status_is_preserved_without_zero_inference() {
    for history in [
        LandUse::Unobserved,
        LandUse::Nonland,
        LandUse::Crop,
        LandUse::Grazing,
        LandUse::Uncleared,
    ] {
        let assessment = assess(
            base(),
            Region::Moderate,
            1000,
            history,
            Reservations::default(),
        )
        .expect("valid policy");
        assert_eq!(assessment.historical_land_use, history);
        assert_eq!(
            assessment.input.eligible,
            matches!(history, LandUse::Unobserved | LandUse::Uncleared)
        );
    }
    assert_eq!(
        assess(
            base(),
            Region::Sparse,
            1001,
            LandUse::Unobserved,
            Reservations::default()
        ),
        Err(EcologyError::SupportOutOfRange)
    );
}

#[test]
fn modern_cover_and_provenance_do_not_create_historical_pnv_or_clearance() {
    let expected = input(base(), LandUse::Unobserved, Reservations::default());
    for modern in [1, 50, 80, 90] {
        let mut tile = base();
        tile.modern_land_cover_class = Some(modern);
        tile.vegetation_provenance = Provenance::Fallback;
        assert_eq!(
            input(tile, LandUse::Unobserved, Reservations::default()),
            expected
        );
    }
}

#[test]
fn physical_constraints_and_treeless_biomes_cannot_get_forest_floor() {
    let patches = Patches::new([17; 32], 1, Parameters::default()).expect("parameters");
    let mut forbidden = Vec::new();
    let mut tile = base();
    tile.water = WaterKind::Ocean;
    forbidden.push(tile);
    let mut tile = base();
    tile.passable = false;
    forbidden.push(tile);
    let mut tile = base();
    tile.surface.kind = SurfaceKind::Cliff;
    forbidden.push(tile);
    for material in [
        GroundMaterial::Rock,
        GroundMaterial::Sand,
        GroundMaterial::Ice,
    ] {
        let mut tile = base();
        tile.material = material;
        forbidden.push(tile);
    }
    for biome in [
        Biome::Steppe,
        Biome::Desert,
        Biome::Tundra,
        Biome::Alpine,
        Biome::Polar,
    ] {
        let mut tile = base();
        tile.biome = biome;
        forbidden.push(tile);
    }
    for tile in forbidden {
        let assessment = input(tile, LandUse::Unobserved, Reservations::default());
        assert!(!assessment.fitness.suitable);
        for x in 0..128 {
            let sample = patches.sample(x, 10, |_, _| assessment);
            assert!(!sample.tree);
            assert_eq!(sample.zone, Zone::Exterior);
            assert_eq!(sample.canopy_per_thousand, 0);
            assert_eq!(sample.forest_floor_per_thousand, 0);
        }
    }
    let mut snow = base();
    snow.biome = Biome::Boreal;
    snow.material = GroundMaterial::Snow;
    assert!(
        input(snow, LandUse::Unobserved, Reservations::default())
            .fitness
            .suitable
    );
}

#[test]
fn shared_exclusions_remove_tree_canopy_and_floor_together_before_filtering() {
    let patches = Patches::new([17; 32], 1, Parameters::default()).expect("parameters");
    for history in [
        LandUse::Crop,
        LandUse::Grazing,
        LandUse::Nonland,
        LandUse::Unobserved,
    ] {
        for reservations in [
            Reservations {
                route: true,
                ..Reservations::default()
            },
            Reservations {
                resource_approach: true,
                ..Reservations::default()
            },
            Reservations {
                start: true,
                ..Reservations::default()
            },
        ] {
            let assessment = input(base(), history, reservations);
            for x in 0..128 {
                let sample = patches.sample(x, 10, |_, _| assessment);
                assert!(!sample.tree);
                assert_eq!(sample.canopy_per_thousand, 0);
                assert_eq!(sample.forest_floor_per_thousand, 0);
            }
        }
    }
    let mut retained = 0;
    for y in 0..128 {
        for x in 0..128 {
            let sample = patches.sample(x, y, |nx, _| {
                input(
                    base(),
                    if nx.rem_euclid(32) < 8 {
                        LandUse::Crop
                    } else {
                        LandUse::Uncleared
                    },
                    Reservations::default(),
                )
            });
            if x % 32 < 8 {
                assert!(!sample.tree);
                assert_eq!(sample.forest_floor_per_thousand, 0);
            } else {
                retained += usize::from(sample.tree);
                assert_eq!(sample.canopy_per_thousand, sample.forest_floor_per_thousand);
            }
        }
    }
    assert!(retained > 100, "mask must not silently remove every forest");
}

#[test]
fn savanna_is_sparse_and_never_receives_forest_floor() {
    let patches = Patches::new([17; 32], 1, Parameters::default()).expect("parameters");
    let mut tile = base();
    tile.biome = Biome::Savanna;
    tile.material = GroundMaterial::DryGrass;
    let assessment = input(tile, LandUse::Uncleared, Reservations::default());
    assert_eq!(assessment.mode, Mode::SparseSavanna);
    let mut trees = 0;
    for y in 0..128 {
        for x in 0..128 {
            let sample = patches.sample(x, y, |_, _| assessment);
            trees += usize::from(sample.tree);
            assert_eq!(sample.forest_floor_per_thousand, 0);
        }
    }
    assert!(trees > 0 && trees < 1640);
}
