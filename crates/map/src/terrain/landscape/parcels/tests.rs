use super::{FractionError, LandUse, Parcels, SourceFractions, hilbert};

const SEEDS: [u64; 16] = [0, 1, 2, 3, 7, 11, 17, 18, 23, 29, 31, 47, 63, 71, 97, 255];

fn fractions(crop: u8, grazing: u8) -> SourceFractions {
    SourceFractions::new(crop, grazing).unwrap()
}

fn close(count: usize, denominator: usize, percent: usize, tolerance: usize) {
    let actual = count * 10_000 / denominator;
    assert!(
        actual.abs_diff(percent * 100) <= tolerance,
        "count={count}, denominator={denominator}, actual={actual}bp, target={}bp",
        percent * 100
    );
}

#[test]
fn validation_combined_cap_and_observation_states_are_distinct() {
    assert_eq!(
        SourceFractions::new(101, 0),
        Err(FractionError::CropOutOfRange)
    );
    assert_eq!(
        SourceFractions::new(0, 255),
        Err(FractionError::GrazingOutOfRange)
    );
    let capped = fractions(80, 70);
    assert_eq!(capped.crop_percent(), 80);
    assert_eq!(capped.grazing_percent(), 20);
    let parcels = Parcels::new([7; 32], 3);
    for valid in [false, true] {
        assert_eq!(parcels.sample(0, 0, None, valid), LandUse::Unobserved);
    }
    assert_eq!(
        parcels.sample(0, 0, Some(fractions(0, 0)), false),
        LandUse::Nonland
    );
    assert_eq!(
        parcels.sample(0, 0, Some(fractions(0, 0)), true),
        LandUse::Uncleared
    );
    for (x, y) in [(0, 0), (-1, 64), (i32::MIN, i32::MAX)] {
        assert_eq!(
            parcels.sample(x, y, Some(fractions(100, 100)), true),
            LandUse::Crop
        );
        assert_eq!(
            parcels.sample(x, y, Some(fractions(0, 100)), true),
            LandUse::Grazing
        );
    }
}

#[test]
fn space_filling_lookup_is_bijective_and_neighbor_connected() {
    for levels in [2, 3] {
        let side = 1 << levels;
        let mut positions = vec![(0_i32, 0_i32); (side * side) as usize];
        let mut seen = vec![false; positions.len()];
        for y in 0..side {
            for x in 0..side {
                let index = hilbert(x, y, levels) as usize;
                assert!(!seen[index]);
                seen[index] = true;
                positions[index] = (x as i32, y as i32);
            }
        }
        assert!(seen.iter().all(|seen| *seen));
        for pair in positions.windows(2) {
            assert_eq!(
                (pair[0].0 - pair[1].0).abs() + (pair[0].1 - pair[1].1).abs(),
                1
            );
        }
    }
}

#[test]
fn native_512_square_fraction_targets_for_all_sixteen_seeds() {
    // Full SOURCE VALID LAND; no forest/biome filtering. Synthetic fixtures do
    // not qualify real HYDE pages or assert exact fractions after correlated clips.
    let cases = [(0, 0), (25, 0), (50, 0), (100, 0), (20, 30), (0, 100)];
    let denominator = 512 * 512;
    for seed in SEEDS {
        let parcels = Parcels::new([19; 32], seed);
        let mut crop = [0; 6];
        let mut grazing = [0; 6];
        for y in 0..512 {
            for x in 0..512 {
                for (index, &(c, g)) in cases.iter().enumerate() {
                    match parcels.sample(x, y, Some(fractions(c, g)), true) {
                        LandUse::Crop => crop[index] += 1,
                        LandUse::Grazing => grazing[index] += 1,
                        LandUse::Uncleared => {}
                        other => panic!("observed valid-land returned {other:?}"),
                    }
                }
            }
        }
        for (index, &(c, g)) in cases.iter().enumerate() {
            // Two percentage points, independently for each seed and category.
            close(crop[index], denominator, usize::from(c), 200);
            close(grazing[index], denominator, usize::from(g), 200);
            if c == 0 {
                assert_eq!(crop[index], 0);
            }
            if g == 0 {
                assert_eq!(grazing[index], 0);
            }
            if c + g == 100 {
                assert_eq!(crop[index] + grazing[index], denominator);
            }
        }
    }
}

#[test]
fn neutral_valid_land_and_forest_subtypes_do_not_change_denominator() {
    let source = Some(fractions(20, 30));
    for seed in SEEDS {
        let parcels = Parcels::new([19; 32], seed);
        let (mut valid, mut crop, mut grazing) = (0, 0, 0);
        let (mut eligible, mut eligible_crop, mut eligible_grazing) = (0, 0, 0);
        let (mut excluded_crop, mut excluded_grazing) = (0, 0);
        for y in 0..512 {
            for x in 0..512 {
                // Fine neutral sampling of every parcel, not rank-correlated.
                let source_valid = (x + y) % 4 != 0;
                let result = parcels.sample(x, y, source, source_valid);
                if !source_valid {
                    assert_eq!(result, LandUse::Nonland);
                    continue;
                }
                valid += 1;
                crop += usize::from(result == LandUse::Crop);
                grazing += usize::from(result == LandUse::Grazing);
                // Stand-in forest subtype/eligibility excludes half of valid
                // land AFTER realization. The descriptor never receives it.
                if x % 2 == 0 {
                    eligible += 1;
                    eligible_crop += usize::from(result == LandUse::Crop);
                    eligible_grazing += usize::from(result == LandUse::Grazing);
                } else {
                    excluded_crop += usize::from(result == LandUse::Crop);
                    excluded_grazing += usize::from(result == LandUse::Grazing);
                }
            }
        }
        assert_eq!(valid, 512 * 512 * 3 / 4);
        assert_eq!(eligible, valid / 2);
        assert_eq!(crop, eligible_crop + excluded_crop);
        assert_eq!(grazing, eligible_grazing + excluded_grazing);
        close(crop, valid, 20, 200);
        close(grazing, valid, 30, 200);
        close(eligible_crop, eligible, 20, 200);
        close(eligible_grazing, eligible, 30, 200);
        // Separate original/eligible denominators and excluded ratios, not a
        // single tree-suppression percentage pretending to conserve source area.
        eprintln!(
            "seed={seed} original_valid={valid} crop_bp={} grazing_bp={}; \
             eligible={eligible} crop_bp={} grazing_bp={}; \
             excluded_bp_of_original={} excluded_crop_bp={} excluded_grazing_bp={}",
            crop * 10_000 / valid,
            grazing * 10_000 / valid,
            eligible_crop * 10_000 / eligible,
            eligible_grazing * 10_000 / eligible,
            (valid - eligible) * 10_000 / valid,
            excluded_crop * 10_000 / valid,
            excluded_grazing * 10_000 / valid
        );
    }
}

#[test]
fn correlated_clip_is_explicitly_not_a_fraction_guarantee() {
    let parcels = Parcels::new([19; 32], 5);
    let source = Some(fractions(20, 30));
    let (mut original, mut original_crop, mut eligible, mut eligible_crop) = (0, 0, 0, 0);
    for y in 0..512 {
        for x in 0..512 {
            let result = parcels.sample(x, y, source, true);
            original += 1;
            original_crop += usize::from(result == LandUse::Crop);
            // Adversarial upstream valid-land clip keeps only crop ranks.
            let valid = result == LandUse::Crop;
            if valid {
                eligible += 1;
                eligible_crop += usize::from(parcels.sample(x, y, source, valid) == LandUse::Crop);
            }
        }
    }
    close(original_crop, original, 20, 200);
    assert_eq!(eligible_crop, eligible);
    assert!(eligible > 0);
    assert!(eligible < original / 4);
}

#[test]
fn clearing_is_spatially_coherent_not_pixel_checkerboard_or_parity() {
    for seed in SEEDS {
        let parcels = Parcels::new([19; 32], seed);
        let source = Some(fractions(20, 30));
        let (mut same_neighbor, mut edges, mut isolated) = (0, 0, 0);
        for y in 1..511 {
            for x in 1..511 {
                let center = parcels.sample(x, y, source, true);
                let neighbors = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
                let matches = neighbors
                    .iter()
                    .filter(|&&(nx, ny)| parcels.sample(nx, ny, source, true) == center)
                    .count();
                same_neighbor += matches;
                edges += 4;
                isolated += usize::from(matches == 0);
            }
        }
        assert!(same_neighbor * 100 > edges * 90);
        assert!(isolated < 50, "seed={seed}, singletons={isolated}");
        let repeated = (0..256)
            .filter(|&x| {
                parcels.sample(x, 21, source, true) == parcels.sample(x + 128, 21, source, true)
            })
            .count();
        assert!(repeated < 240, "8x8 parcel parity repetition seed={seed}");
    }
}

#[test]
fn deterministic_world_coordinates_chunk_seams_and_extremes() {
    let first = Parcels::new([1; 32], 4);
    let identical = Parcels::new([1; 32], 4);
    let seed_changed = Parcels::new([1; 32], 5);
    let key_changed = Parcels::new([2; 32], 4);
    let source = Some(fractions(25, 25));
    let (mut seed_different, mut key_different) = (0, 0);
    for y in -80..80 {
        for x in -80..80 {
            let value = first.sample(x, y, source, true);
            assert_eq!(value, identical.sample(x, y, source, true));
            // Reconstruction from chunk-local coordinates agrees on negative
            // seams too; no descriptor origin, halo or query-order state.
            let world_x = x.div_euclid(32) * 32 + x.rem_euclid(32);
            let world_y = y.div_euclid(32) * 32 + y.rem_euclid(32);
            assert_eq!(value, first.sample(world_x, world_y, source, true));
            seed_different += usize::from(value != seed_changed.sample(x, y, source, true));
            key_different += usize::from(value != key_changed.sample(x, y, source, true));
        }
    }
    assert!(seed_different > 1000);
    assert!(key_different > 1000);
    for x in [i32::MIN, -513, -512, -1, 0, 511, 512, i32::MAX] {
        for y in [i32::MIN, -65, -64, -1, 0, 63, 64, i32::MAX] {
            assert_eq!(
                first.sample(x, y, source, true),
                identical.sample(x, y, source, true)
            );
        }
    }
}
