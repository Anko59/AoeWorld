use super::*;
use crate::landscape_patches::{Opening, Patches, Zone};
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
    assess(tile, 1000, history, reservations)
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
        let assessment =
            assess(base(), 1000, history, Reservations::default()).expect("valid policy");
        assert_eq!(assessment.historical_land_use, history);
        // Crop and grazing parcels set regional wildness upstream; they no
        // longer punch parcel-shaped holes into stands.
        assert_eq!(assessment.input.eligible, history != LandUse::Nonland);
    }
    assert_eq!(
        assess(base(), 1001, LandUse::Unobserved, Reservations::default()),
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
    let patches = Patches::new([17; 32], 1);
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
            let sample = patches.sample(x, 10, |_, _| assessment, |_, _| 1000);
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
    let patches = Patches::new([17; 32], 1);
    for (history, reservations) in [
        (LandUse::Nonland, Reservations::default()),
        (
            LandUse::Unobserved,
            Reservations {
                resource_approach: true,
                ..Reservations::default()
            },
        ),
        (
            LandUse::Crop,
            Reservations {
                start: true,
                ..Reservations::default()
            },
        ),
    ] {
        let assessment = input(base(), history, reservations);
        assert!(!assessment.eligible);
        for x in 0..128 {
            let sample = patches.sample(x, 10, |_, _| assessment, |_, _| 1000);
            assert!(!sample.tree);
            assert_eq!(sample.canopy_per_thousand, 0);
            assert_eq!(sample.forest_floor_per_thousand, 0);
        }
    }
    let mut retained = 0;
    for y in 0..128 {
        for x in 0..128 {
            let sample = patches.sample(
                x,
                y,
                |nx, _| {
                    let reservations = Reservations {
                        start: nx.rem_euclid(32) < 8,
                        ..Reservations::default()
                    };
                    input(base(), LandUse::Uncleared, reservations)
                },
                |_, _| 1000,
            );
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
fn crop_and_grazing_parcels_do_not_cut_parcel_shaped_holes_into_stands() {
    let patches = Patches::new([17; 32], 1);
    let parcels = crate::historical_parcels::Parcels::new([17; 32], 1);
    let fractions = crate::historical_parcels::SourceFractions::new(40, 20).unwrap();
    let (mut farmed, mut canopy) = (0, 0);
    for y in 0..256 {
        for x in 0..256 {
            let history = parcels.sample(x, y, Some(fractions), true);
            let with_history = patches.sample(
                x,
                y,
                |nx, ny| {
                    let history = parcels.sample(nx, ny, Some(fractions), true);
                    input(base(), history, Reservations::default())
                },
                |_, _| 1000,
            );
            let uncleared = input(base(), LandUse::Uncleared, Reservations::default());
            assert_eq!(
                with_history,
                patches.sample(x, y, |_, _| uncleared, |_, _| 1000)
            );
            farmed += usize::from(matches!(history, LandUse::Crop | LandUse::Grazing));
            canopy += usize::from(with_history.canopy_per_thousand > 0);
        }
    }
    assert!(farmed > 20_000 && canopy > 20_000);
}

#[test]
fn routes_are_corridors_cleared_only_inside_large_forests() {
    let patches = Patches::new([17; 32], 1);
    let route = Reservations {
        route: true,
        ..Reservations::default()
    };
    let corridor = input(base(), LandUse::Uncleared, route);
    assert!(corridor.eligible && corridor.corridor);
    let plain = input(base(), LandUse::Uncleared, Reservations::default());
    let (mut cleared, mut kept) = (0, 0);
    for y in 0..384 {
        for x in 0..384 {
            // Groves only: a route changes nothing.
            assert_eq!(
                patches.sample(x, y, |_, _| corridor, |_, _| 300),
                patches.sample(x, y, |_, _| plain, |_, _| 300)
            );
            let large = patches.sample(x, y, |_, _| corridor, |_, _| 1000);
            if patches
                .sample(x, y, |_, _| plain, |_, _| 1000)
                .canopy_per_thousand
                > 0
            {
                if large.opening == Opening::Trail {
                    cleared += 1;
                    assert_eq!((large.tree, large.canopy_per_thousand), (false, 0));
                } else {
                    kept += 1;
                }
            }
        }
    }
    assert!(cleared > 10_000, "routes clear large forests: {cleared}");
    assert!(
        kept > 1_000,
        "groves beside large forests stay solid: {kept}"
    );
}

#[test]
fn savanna_is_sparse_and_never_receives_forest_floor() {
    let patches = Patches::new([17; 32], 1);
    let mut tile = base();
    tile.biome = Biome::Savanna;
    tile.material = GroundMaterial::DryGrass;
    let assessment = input(tile, LandUse::Uncleared, Reservations::default());
    assert_eq!(assessment.mode, Mode::SparseSavanna);
    let mut trees = 0;
    for y in 0..128 {
        for x in 0..128 {
            let sample = patches.sample(x, y, |_, _| assessment, |_, _| 1000);
            trees += usize::from(sample.tree);
            assert_eq!(sample.forest_floor_per_thousand, 0);
        }
    }
    assert!(trees > 0 && trees < 1640);
}
