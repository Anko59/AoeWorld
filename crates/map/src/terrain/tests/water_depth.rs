//! Water depth classes by distance from land, and coherent ground where the
//! potential-vegetation source has no class. Reaches are modelling choices.
use super::shoreline::{Pages, WIDTH, at, bind, coast, composed, environment, pyramid, wet};
use super::*;
use crate::{EnvironmentPage, LandscapeTile};

/// Shallow rim: land within one tile, or two where the rim widens.
const RIM: i32 = 5;
/// Medium water ends five to seven tiles from land.
const MEDIUM_NARROW: i32 = 25;
const MEDIUM_WIDE: i32 = 50;

/// Squared distance to the nearest land tile within eight tiles.
fn nearest_land(tiles: &[LandscapeTile], x: i32, y: i32) -> Option<i32> {
    let mut nearest: Option<i32> = None;
    for dy in -8..=8_i32 {
        for dx in -8..=8_i32 {
            if at(tiles, x + dx, y + dy).is_some() && !wet(tiles, x + dx, y + dy) {
                let distance = dx * dx + dy * dy;
                nearest = Some(nearest.map_or(distance, |best| best.min(distance)));
            }
        }
    }
    nearest
}

fn big_lake(key: u8) -> MapChunkGenerator {
    coast(key, &|sx, sy| {
        let lake = (6..=26).contains(&sx) && (6..=26).contains(&sy);
        (0, if lake { 100 } else { 0 })
    })
}

#[test]
fn open_water_grades_from_a_shallow_rim_through_medium_to_deep() {
    for key in [61, 62] {
        let generator = big_lake(key);
        let tiles = composed(&generator);
        let (mut shallow, mut medium, mut deep) = (0, 0, 0);
        for y in 0..WIDTH {
            for x in 0..WIDTH {
                let tile = at(&tiles, x, y).expect("tile").terrain;
                if tile.water == WaterKind::None {
                    continue;
                }
                assert!(!tile.passable, "open water is never walkable at {x},{y}");
                let land = nearest_land(&tiles, x, y);
                match (tile.water, tile.material) {
                    (WaterKind::Shallow, GroundMaterial::Shore) => {
                        shallow += 1;
                        assert!(land.is_some_and(|land| land <= RIM), "rim at {x},{y}");
                    }
                    (WaterKind::Lake, GroundMaterial::Water) => {
                        medium += 1;
                        assert!(
                            land.is_some_and(|land| land > 2 && land <= MEDIUM_WIDE),
                            "medium water {land:?} from land at {x},{y}"
                        );
                    }
                    (WaterKind::Lake, GroundMaterial::DeepWater) => {
                        deep += 1;
                        assert!(
                            land.is_none_or(|land| land > MEDIUM_NARROW),
                            "deep water {land:?} from land at {x},{y}"
                        );
                    }
                    other => panic!("unexpected water {other:?} at {x},{y}"),
                }
            }
        }
        assert!(
            shallow > 300 && medium > 1_000,
            "key {key}: {shallow} {medium}"
        );
        assert!(deep > medium, "key {key}: {deep} deep, {medium} medium");
    }
}

#[test]
fn point_queries_agree_with_chunks_on_every_depth_class() {
    let generator = big_lake(63);
    let tiles = composed(&generator);
    let mut classes = std::collections::BTreeSet::new();
    for (x, y) in (30..230)
        .map(|x| (x, 128))
        .chain((30..230).map(|y| (97, y)))
    {
        let point = generator
            .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
            .expect("point query")
            .expect("inside the map");
        let chunk = at(&tiles, x, y).expect("tile").terrain;
        assert_eq!(
            (point.tile.terrain.water, point.tile.terrain.material),
            (chunk.water, chunk.material),
            "point and chunk disagree at {x},{y}"
        );
        classes.insert(chunk.material as u8);
    }
    for material in [
        GroundMaterial::Shore,
        GroundMaterial::Water,
        GroundMaterial::DeepWater,
    ] {
        assert!(classes.contains(&(material as u8)), "{material:?} sampled");
    }
}

#[test]
fn rivers_are_shallow_at_the_bank_medium_midstream_and_never_deep() {
    let tiles = composed(&super::shore_band::modeled_river(84));
    let (mut edge, mut middle) = (0, 0);
    for y in 0..WIDTH {
        for x in 0..WIDTH {
            let tile = at(&tiles, x, y).expect("tile").terrain;
            if tile.water != WaterKind::River {
                continue;
            }
            let bank = nearest_land(&tiles, x, y).is_some_and(|land| land <= 2);
            if bank {
                edge += 1;
                assert_eq!(
                    tile.material,
                    GroundMaterial::Shore,
                    "bank water at {x},{y}"
                );
            } else {
                middle += 1;
                assert_eq!(tile.material, GroundMaterial::Water, "midstream at {x},{y}");
            }
        }
    }
    assert!(
        edge > 100 && middle > 100,
        "{edge} bank, {middle} midstream"
    );
}

/// Temperate potential vegetation with a five-by-five hole of unclassified
/// (class 0) cells: 33 samples over the 257-tile fixture, eight tiles each.
fn vegetation_hole(key: u8) -> MapChunkGenerator {
    let (mut environment, mut pages) = environment(33, 8, &|_, _| (0, 0));
    let class = |sx: u16, sy: u16| {
        if (14..=18).contains(&sx) && (14..=18).contains(&sy) {
            0
        } else {
            8
        }
    };
    let page = EnvironmentPage::Vegetation(PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        potential_biome_class: (0..33_u16)
            .flat_map(|sy| (0..33_u16).map(move |sx| class(sx, sy)))
            .collect(),
    });
    pages.0.insert(page.key(), Arc::new(page));
    environment.vegetation = Some(pyramid(33));
    bind(key, environment, Pages(pages.0))
}

#[test]
fn unclassified_vegetation_cells_take_the_nearest_classified_biome_not_seeded_noise() {
    for key in [71, 72, 73] {
        let generator = vegetation_hole(key);
        for y in (14 * 8 - 4)..(19 * 8 - 4) {
            for x in (14 * 8 - 4)..(19 * 8 - 4) {
                let tile = generator.tile_at(TileCoord::new(x, y)).expect("tile");
                assert_eq!(
                    (tile.biome, tile.material, tile.passable),
                    (Biome::Temperate, GroundMaterial::TemperateGrass, true),
                    "key {key}: seeded biome noise inside a source gap at {x},{y}"
                );
            }
        }
    }
}
