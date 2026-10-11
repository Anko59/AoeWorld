use super::*;
use GroundMaterial::{
    Dirt, DryGrass, ForestFloor, GrassDirt, Gravel, Ice, LushGrass, Mud, Rock, Sand, Shore, Snow,
    TemperateGrass, Water, WornGrass,
};

const SIDE: i32 = 256;
const SEEDS: [u64; 3] = [1, 0x5eed_cafe, u64::MAX / 3];
const LOW: NativeHeightBand = NativeHeightBand::Lowland;

/// A square of one base material, row-major, away from the origin and from
/// any chunk-aligned corner.
fn field(seed: u64, material: GroundMaterial, band: NativeHeightBand) -> Vec<GroundMaterial> {
    (0..SIDE)
        .flat_map(|y| {
            (0..SIDE).map(move |x| {
                let tile = TileCoord::new(4_013 + x, -9_107 + y);
                vary(seed, tile, material, band, LandUse::Uncleared)
            })
        })
        .collect()
}

fn share(field: &[GroundMaterial], material: GroundMaterial) -> f64 {
    field.iter().filter(|tile| **tile == material).count() as f64 / field.len() as f64
}

#[test]
fn grassland_mixes_related_terrains_in_natural_proportions() {
    for seed in SEEDS {
        let field = field(seed, TemperateGrass, LOW);
        // A dominant base with worn rings, grass and dirt mix inside them and
        // small bare cores, plus broad darker patches: none is a rarity.
        for (material, range) in [
            (TemperateGrass, 0.45..0.82),
            (WornGrass, 0.07..0.28),
            (GrassDirt, 0.03..0.16),
            (Dirt, 0.004..0.06),
            (LushGrass, 0.03..0.20),
        ] {
            let actual = share(&field, material);
            assert!(
                range.contains(&actual),
                "{material:?} {actual} (seed {seed})"
            );
        }
    }
}

#[test]
fn every_family_keeps_its_own_climate() {
    let families: [(GroundMaterial, &[GroundMaterial]); 5] = [
        (
            TemperateGrass,
            &[TemperateGrass, WornGrass, GrassDirt, Dirt, LushGrass],
        ),
        (LushGrass, &[LushGrass, TemperateGrass, WornGrass]),
        (DryGrass, &[DryGrass, GrassDirt, Dirt, WornGrass]),
        (Sand, &[Sand, Gravel, Dirt]),
        (Rock, &[Rock, Gravel, Dirt]),
    ];
    for seed in SEEDS {
        for (base, family) in families {
            let field = field(seed, base, LOW);
            assert!(field.iter().all(|tile| family.contains(tile)), "{base:?}");
            // Every member really occurs and the base stays the main ground.
            assert!(
                family.iter().all(|member| share(&field, *member) > 0.0),
                "{base:?}"
            );
            assert!(share(&field, base) > 0.45, "{base:?}");
        }
        // Forest floor and every physically meaningful ground stay as they are.
        for fixed in [
            ForestFloor,
            Dirt,
            Mud,
            Snow,
            Ice,
            Shore,
            Water,
            GrassDirt,
            WornGrass,
            Gravel,
        ] {
            assert!(
                field(seed, fixed, LOW).iter().all(|tile| *tile == fixed),
                "{fixed:?}"
            );
        }
    }
}

#[test]
fn patches_are_blobs_of_a_few_to_a_few_dozen_tiles_not_speckle() {
    for seed in SEEDS {
        let field = field(seed, TemperateGrass, LOW);
        let at = |x: i32, y: i32| field[(y * SIDE + x) as usize];
        let varied = |x: i32, y: i32| !matches!(at(x, y), TemperateGrass | LushGrass);
        // Connected worn patches by flood fill over four neighbours.
        let mut seen = vec![false; field.len()];
        let mut sizes = Vec::new();
        for start in 0..field.len() as i32 {
            let (x, y) = (start % SIDE, start / SIDE);
            if seen[start as usize] || !varied(x, y) {
                continue;
            }
            let mut stack = vec![(x, y)];
            seen[start as usize] = true;
            let mut size = 0_usize;
            while let Some((x, y)) = stack.pop() {
                size += 1;
                for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                    if (0..SIDE).contains(&nx)
                        && (0..SIDE).contains(&ny)
                        && varied(nx, ny)
                        && !seen[(ny * SIDE + nx) as usize]
                    {
                        seen[(ny * SIDE + nx) as usize] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            sizes.push(size);
        }
        sizes.sort_unstable();
        let tiles = sizes.iter().sum::<usize>();
        let specks = sizes.iter().filter(|size| **size <= 2).sum::<usize>();
        let median = sizes[sizes.len() / 2];
        let largest = sizes[sizes.len() - 1];
        assert!(sizes.len() >= 40, "only {} patches", sizes.len());
        assert!(
            specks * 25 < tiles,
            "speckle {specks}/{tiles} (seed {seed})"
        );
        assert!(
            (4..=40).contains(&median),
            "median patch {median} (seed {seed})"
        );
        // Worn rings may join neighbours, never into one sheet over the field.
        assert!(
            largest * 12 < field.len(),
            "one patch covers {largest} tiles"
        );
    }
}

#[test]
fn patch_edges_ignore_chunk_and_power_of_two_grids() {
    let field = field(SEEDS[1], TemperateGrass, LOW);
    for period in [8, 16, 32, 64] {
        for vertical in [false, true] {
            let (mut on, mut lines_on, mut off, mut lines_off) = (0_u32, 0_u32, 0_u32, 0_u32);
            for line in 1..SIDE {
                let world = line + if vertical { 4_013 } else { -9_107 };
                let changes = (0..SIDE)
                    .filter(|along| {
                        let index = |line: i32| {
                            (if vertical {
                                along * SIDE + line
                            } else {
                                line * SIDE + along
                            }) as usize
                        };
                        field[index(line)] != field[index(line - 1)]
                    })
                    .count() as u32;
                if world.rem_euclid(period) == 0 {
                    on += changes;
                    lines_on += 1;
                } else {
                    off += changes;
                    lines_off += 1;
                }
            }
            // Edges crossing a grid line are as frequent as anywhere else.
            let (on, off) = (on * lines_off, off * lines_on);
            assert!(
                on * 10 < off * 14 && off * 10 < on * 14,
                "period {period}: {on} vs {off}"
            );
        }
    }
}

#[test]
fn variety_is_a_pure_function_of_seed_and_world_tile() {
    let first = field(SEEDS[0], DryGrass, LOW);
    assert_eq!(first, field(SEEDS[0], DryGrass, LOW));
    assert_ne!(first, field(SEEDS[1], DryGrass, LOW));
    assert_ne!(seed([1; 32], 7), seed([2; 32], 7));
    assert_ne!(seed([1; 32], 7), seed([1; 32], 8));
    // Extreme coordinates neither overflow nor leave the family.
    for tile in [
        TileCoord::new(i32::MIN, i32::MIN),
        TileCoord::new(i32::MAX, i32::MIN),
        TileCoord::new(i32::MAX, i32::MAX),
        TileCoord::new(-1, 0),
    ] {
        let varied = vary(SEEDS[2], tile, Sand, LOW, LandUse::Uncleared);
        assert!(matches!(varied, Sand | Gravel | Dirt));
    }
}

#[test]
fn worked_and_high_ground_is_more_worn_and_breaks_to_gravel() {
    let worn = |band, land_use| {
        (0..SIDE * SIDE)
            .filter(|index| {
                let tile = TileCoord::new(index % SIDE - 70, index / SIDE + 300);
                !matches!(
                    vary(SEEDS[0], tile, TemperateGrass, band, land_use),
                    TemperateGrass | LushGrass
                )
            })
            .count()
    };
    let open = worn(LOW, LandUse::Uncleared);
    assert!(worn(LOW, LandUse::Grazing) > open);
    assert!(worn(LOW, LandUse::Crop) > worn(LOW, LandUse::Grazing));
    assert!(worn(NativeHeightBand::Subalpine, LandUse::Uncleared) > open);
    let high = field(SEEDS[0], TemperateGrass, NativeHeightBand::Subalpine);
    assert!(share(&high, Gravel) > 0.0 && share(&high, Dirt) == 0.0);
    assert_eq!(share(&field(SEEDS[0], TemperateGrass, LOW), Gravel), 0.0);
}

#[test]
fn high_band_dry_grass_breaks_to_gravel_never_bare_dirt() {
    // The subalpine rule is one rule for every grass: worn through, the
    // ground above the treeline is gravel, not bare soil.
    for seed in SEEDS {
        let field = field(seed, DryGrass, NativeHeightBand::Subalpine);
        assert_eq!(
            share(&field, Dirt),
            0.0,
            "bare dirt above the treeline (seed {seed})"
        );
        assert!(
            share(&field, Gravel) > 0.0,
            "no gravel on worn high ground (seed {seed})"
        );
    }
}

#[test]
fn all_high_band_grass_families_break_their_most_worn_patches_to_gravel() {
    for seed in SEEDS {
        for base in [TemperateGrass, DryGrass, LushGrass] {
            let low = field(seed, base, LOW);
            assert_eq!(share(&low, Gravel), 0.0, "{base:?} low band");
            for band in [
                NativeHeightBand::Subalpine,
                NativeHeightBand::Alpine,
                NativeHeightBand::Nival,
            ] {
                let high = field(seed, base, band);
                assert_eq!(share(&high, Dirt), 0.0, "{base:?} {band:?}");
                assert!(share(&high, Gravel) > 0.0, "{base:?} {band:?}");
                for (index, actual) in high.iter().enumerate() {
                    let tile =
                        TileCoord::new(4_013 + index as i32 % SIDE, -9_107 + index as i32 / SIDE);
                    // Every family uses the same high-band wear bias and threshold; the
                    // most-worn class changes (lush grass formerly used worn grass).
                    let worn = (wear(seed, tile) + 250) * 1000 / UNIT;
                    let threshold = 770;
                    assert_eq!(*actual == Gravel, worn >= threshold, "{base:?} {tile:?}");
                }
            }
        }
    }
}

#[test]
fn every_base_material_keeps_its_sampled_output() {
    // Pins the drawn sheets against accidental change while the selection
    // internals move: a digest over a fixed low-band sample, where every
    // base material's current output is the intended one.
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    for base in [TemperateGrass, DryGrass, LushGrass, Sand, Rock] {
        for y in (0..SIDE).step_by(3) {
            for x in (0..SIDE).step_by(3) {
                let tile = TileCoord::new(4_013 + x, -9_107 + y);
                let varied = vary(SEEDS[0], tile, base, LOW, LandUse::Uncleared);
                for byte in format!("{varied:?}").into_bytes() {
                    digest = (digest ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
    }
    assert_eq!(format!("{digest:016x}"), "21a6efa9d5326472");
}

#[test]
fn high_band_gravel_share_is_identical_across_grass_families() {
    // Paired tiles use the same wear field: there is no independent sampling
    // noise. A zero-tile (zero share) tolerance is therefore justified.
    for seed in 0..12 {
        for band in [NativeHeightBand::Subalpine, NativeHeightBand::Alpine, NativeHeightBand::Nival] {
            for land_use in [LandUse::Uncleared, LandUse::Grazing, LandUse::Crop] {
                let mut counts = [0; 3];
                for y in 0..128 {
                    for x in 0..128 {
                        let tile = TileCoord::new(4_013 + x, -9_107 + y);
                        let gravel = [TemperateGrass, DryGrass, LushGrass].map(|base| {
                            vary(seed, tile, base, band, land_use) == Gravel
                        });
                        assert_eq!(gravel, [gravel[0]; 3], "seed {seed} {band:?} {land_use:?} {tile:?}");
                        for (count, present) in counts.iter_mut().zip(gravel) {
                            *count += usize::from(present);
                        }
                    }
                }
                assert!(counts[0] > 0 && counts[0] < 128 * 128);
                assert_eq!(counts, [counts[0]; 3]);
            }
        }
    }
}

#[path = "variety_cost.rs"]
pub(super) mod cost;
