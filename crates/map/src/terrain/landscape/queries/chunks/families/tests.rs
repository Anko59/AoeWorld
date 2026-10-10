use super::*;

const KEY: [u8; 32] = [37; 32];
const BIOMES: [Biome; 10] = [
    Biome::Temperate,
    Biome::Boreal,
    Biome::Tropical,
    Biome::Woodland,
    Biome::Savanna,
    Biome::Steppe,
    Biome::Desert,
    Biome::Tundra,
    Biome::Alpine,
    Biome::Polar,
];

fn mixed(key: [u8; 32], seed: u64, x: i32, y: i32) -> ResourceVisualFamily {
    source_tree_family(key, seed, Some(9), Biome::Temperate, TileCoord::new(x, y))
}

#[test]
fn locked_source_classes_and_missing_classes_use_the_biome_fallback() {
    // Locked semantic classes, not an external-source histogram or a CSV
    // parser/provenance verification test. Mixed class 9 is tested below.
    for (class, expected) in [
        (8, ResourceVisualFamily::Conifer), // cool evergreen needleleaf forest
        (13, ResourceVisualFamily::Broadleaf), // temperate deciduous broadleaf
        (15, ResourceVisualFamily::Conifer), // cold evergreen needleleaf forest
        (17, ResourceVisualFamily::Conifer), // temperate needleleaf open woodland
    ] {
        for biome in BIOMES {
            for seed in [0, 1, u64::MAX] {
                assert_eq!(
                    source_tree_family(KEY, seed, Some(class), biome, TileCoord::new(-17, 31)),
                    expected,
                );
            }
        }
    }
    for biome in BIOMES {
        let expected = match biome {
            Biome::Boreal => ResourceVisualFamily::Conifer,
            Biome::Tropical => ResourceVisualFamily::Tropical,
            Biome::Woodland | Biome::Savanna => ResourceVisualFamily::DryScrub,
            _ => ResourceVisualFamily::Broadleaf,
        };
        assert_eq!(tree_family(biome), expected);
        // Class 14 (cold deciduous, possibly larch) deliberately falls back.
        for class in (0..=u8::MAX).filter(|class| !matches!(*class, 8 | 9 | 13 | 15 | 17)) {
            assert_eq!(
                source_tree_family(
                    KEY,
                    5,
                    Some(class),
                    biome,
                    TileCoord::new(i32::MIN, i32::MAX)
                ),
                expected,
            );
        }
        assert_eq!(
            source_tree_family(KEY, 5, None, biome, TileCoord::new(-1, -1)),
            expected
        );
    }
}

#[test]
fn mixed_stands_are_euclidean_correlated_and_order_independent_at_signed_extremes() {
    // Every full stand has one family, including either side of zero. Cell
    // origins below safely admit all offsets without saturating/wrapping.
    for origin in [i32::MIN, -32, -16, 0, 16, i32::MAX - 15] {
        for other in [i32::MIN, -16, 0, i32::MAX - 15] {
            for seed in [0, 1, 73, u64::MAX] {
                let expected = mixed(KEY, seed, origin, other);
                for y in 0..16 {
                    for x in 0..16 {
                        assert_eq!(mixed(KEY, seed, origin + x, other + y), expected);
                    }
                }
            }
        }
    }
    let points: Vec<_> = (-65..=65)
        .map(|x| TileCoord::new(x, x * 3 - 7))
        .chain([
            TileCoord::new(i32::MIN, i32::MAX),
            TileCoord::new(i32::MAX, i32::MIN),
        ])
        .collect();
    for seed in [0, 1, 73, u64::MAX] {
        let forward: Vec<_> = points.iter().map(|p| mixed(KEY, seed, p.x, p.y)).collect();
        let mut reverse: Vec<_> = points
            .iter()
            .rev()
            .map(|p| mixed(KEY, seed, p.x, p.y))
            .collect();
        reverse.reverse();
        assert_eq!(forward, reverse);
        // Pure function partitioning; generator partitioning is tested separately.
        let partitioned: Vec<_> = points
            .chunks(32)
            .flat_map(|chunk| chunk.iter().map(|p| mixed(KEY, seed, p.x, p.y)))
            .collect();
        assert_eq!(forward, partitioned);
    }
}

#[test]
fn mixed_generic_regions_have_both_families_seed_variation_and_non_checkerboard_neighbors() {
    let mut first = Vec::new();
    let mut seed_changed = false;
    let mut key_changed = false;
    for seed in [0, 1, 2, 7, 19, 73, 4096, u64::MAX] {
        let mut conifers = 0;
        let mut same_neighbors = 0;
        let mut neighbors = 0;
        let mut stands = Vec::new();
        for y in -16..16 {
            for x in -16..16 {
                let family = mixed(KEY, seed, x * 16, y * 16);
                assert!(matches!(
                    family,
                    ResourceVisualFamily::Broadleaf | ResourceVisualFamily::Conifer
                ));
                conifers += usize::from(family == ResourceVisualFamily::Conifer);
                stands.push(family);
                key_changed |= family != mixed([91; 32], seed, x * 16, y * 16);
            }
        }
        // Model expectation is 50/50, not an exact count or observed botanical
        // proportion. Loose generic bounds reject one-family/select-by-parity.
        assert!(
            (307..=717).contains(&conifers),
            "seed {seed}: {conifers}/1024"
        );
        if first.is_empty() {
            first = stands;
        } else {
            seed_changed |= first != stands;
        }
        for y in -128..128 {
            for x in -128..128 {
                let family = mixed(KEY, seed, x, y);
                for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                    same_neighbors += usize::from(family == mixed(KEY, seed, nx, ny));
                    neighbors += 1;
                }
            }
        }
        assert!(same_neighbors * 100 >= neighbors * 93);
    }
    assert!(seed_changed);
    assert!(key_changed);
}
