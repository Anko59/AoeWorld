//! A sub-cell river crossing the entire flat map must leave both banks reachable.
use super::shoreline::{Pages, WIDTH, at, bind, composed, environment};
use super::*;
use crate::{EnvironmentPage, GroundMaterial, HydrologyEvidencePage, WaterKind};

fn crossing() -> MapChunkGenerator {
    crossing_with_coast(false)
}

fn crossing_with_coast(coast: bool) -> MapChunkGenerator {
    crossing_with_relief(coast, false)
}

fn crossing_with_relief(coast: bool, steep: bool) -> MapChunkGenerator {
    crossing_with_geometry(
        coast,
        steep,
        vec![
            [129 * 256, -256, 137 * 256, 128 * 256, 6 * 256, 0, 129 * 256],
            [
                137 * 256,
                128 * 256,
                129 * 256,
                258 * 256,
                6 * 256,
                129 * 256,
                259 * 256,
            ],
        ],
    )
}

fn crossing_with_geometry(coast: bool, steep: bool, segments: Vec<[i64; 7]>) -> MapChunkGenerator {
    let height = |_: i32, y: i32| y * 600_000;
    crossing_with_height(coast, if steep { Some(&height) } else { None }, segments)
}

fn crossing_with_height(
    coast: bool,
    height: Option<&dyn Fn(i32, i32) -> i32>,
    segments: Vec<[i64; 7]>,
) -> MapChunkGenerator {
    let (mut environment, mut pages) =
        environment(33, 8, &|_, y| (if coast && y >= 28 { 100 } else { 0 }, 0));
    if let Some(height) = height {
        environment.samples_per_axis = 33;
        environment.elevation = super::shoreline::pyramid(33);
        let page = EnvironmentPage::Elevation(crate::ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 33,
            height: 33,
            geographic_height_centimeters: (0..33)
                .flat_map(|y| (0..33).map(move |x| height(x, y)))
                .collect(),
        });
        pages.0.insert(page.key(), Arc::new(page));
    }
    // Geometry is deliberately between evidence cell centres; changing the
    // raster axis must not change its playable width or remove the channel.
    let segments = segments
        .into_iter()
        .map(|s| {
            [
                s[0],
                s[1],
                s[2],
                s[3],
                s[4],
                s[5],
                s[6],
                s[2] - s[0],
                s[3] - s[1],
                s[6] - s[5],
            ]
        })
        .collect::<Vec<_>>();
    let mut value = serde_json::json!({
        "level": 0, "x": 0, "y": 0, "width": 33, "height": 33,
        "kind": vec![crate::HydrologyKind::NoEvidence as u8; 1089],
        "method": vec![0_u8; 1089],
        "river_segments": segments
    });
    // Read through the public package representation, so the test first
    // fails on the missing format and later exercises hashing/validation.
    let page: HydrologyEvidencePage = serde_json::from_value(value.take()).expect("river geometry");
    page.validate().expect("valid river geometry");
    let root = crate::ordered_hydrology_page_root(std::slice::from_ref(&page)).unwrap();
    let page = EnvironmentPage::HydrologyEvidence(page);
    pages.0.insert(page.key(), Arc::new(page));
    let cover = EnvironmentPage::ModernLandCover(crate::ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        worldcover_class: vec![0; 1089],
    });
    pages.0.insert(cover.key(), Arc::new(cover));
    environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
        samples_per_axis: 33,
        page_samples: 64,
        world_cover_year: 2021,
        policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: root,
        modern_land_cover_page_root: [4; 32],
        water_model: None,
    });
    bind(91, environment, Pages(pages.0))
}

#[test]
fn source_river_water_never_invents_invisible_crossings() {
    let generator = crossing();
    let tiles = composed(&generator);
    assert!(tiles.iter().any(|p| p.terrain.water == WaterKind::River));
    for point in tiles.iter().filter(|p| p.terrain.water != WaterKind::None) {
        assert!(!point.terrain.passable, "water at {:?}", point.tile);
        assert_ne!(point.terrain.material, GroundMaterial::Ford);
    }
    let bank = |center_x: i32| {
        tiles
            .iter()
            .filter(|p| p.tile.x.abs_diff(center_x) <= 10 && p.tile.y.abs_diff(128) <= 10)
            .find(|p| p.terrain.passable && generator.object_at(p.tile).is_none())
            .expect("open bank tile")
            .tile
    };
    let route = crate::find_path(&generator, bank(100), bank(160), (WIDTH * WIDTH) as u32);
    assert!(
        matches!(route, crate::MovementOutcome::Unreachable),
        "{route:?}"
    );
}

#[test]
fn subcell_meandering_river_blocks_water_and_keeps_each_bank_connected() {
    let generator = crossing();
    let tiles = composed(&generator);
    let mut river_columns = std::collections::BTreeSet::new();
    for y in 0..WIDTH {
        let river = (0..WIDTH)
            .filter(|x| at(&tiles, *x, y).unwrap().terrain.water == WaterKind::River)
            .collect::<Vec<_>>();
        assert!(
            (5..=7).contains(&river.len()),
            "width at row {y}: {river:?}"
        );
        river_columns.insert(river[0]);
        for x in river {
            let point = at(&tiles, x, y).unwrap();
            assert_ne!(point.terrain.material, GroundMaterial::Ford);
            assert!(!point.terrain.passable);
        }
    }
    assert!(river_columns.len() >= 7, "river follows the source bends");
    let mut seen = std::collections::BTreeSet::from([(0, 0), (WIDTH - 1, WIDTH - 1)]);
    let mut queue = std::collections::VecDeque::from([(0, 0), (WIDTH - 1, WIDTH - 1)]);
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if at(&tiles, nx, ny).is_some_and(|p| p.terrain.passable) && seen.insert((nx, ny)) {
                queue.push_back((nx, ny));
            }
        }
    }
    for point in tiles.iter().filter(|p| p.terrain.passable) {
        assert!(
            seen.contains(&(point.tile.x, point.tile.y)),
            "unreachable {:?}",
            point.tile
        );
    }
    assert!(seen.contains(&(WIDTH - 1, WIDTH - 1)));
}

#[test]
fn source_channel_connects_to_the_shaped_sea_at_its_mouth() {
    let generator = crossing_with_coast(true);
    let tiles = composed(&generator);
    let connected = super::shoreline::main_water(&tiles);
    assert!(connected[129], "river joins the sea component");
    assert!(
        tiles
            .iter()
            .any(|p| connected[(p.tile.y * WIDTH + p.tile.x) as usize]
                && p.terrain.water == WaterKind::Ocean)
    );
    assert!(
        tiles
            .iter()
            .any(|p| connected[(p.tile.y * WIDTH + p.tile.x) as usize]
                && p.terrain.water == WaterKind::River)
    );
}

#[test]
fn river_width_crosses_evidence_page_edges_without_a_cut_or_query_order_change() {
    let (mut environment, mut pages) = environment(129, 8, &|_, _| (0, 0));
    for (x, y, width, height) in super::shoreline::page_grid(129) {
        let len = usize::from(width) * usize::from(height);
        // Persist the same source line split at each page's five-tile halo.
        let start_y = ((i32::from(y) * 64 * WIDTH * 256 + 64) / 129 - 5 * 256).max(0);
        let end_y = (((i32::from(y) * 64 + i32::from(height)) * WIDTH * 256 + 64) / 129 + 5 * 256)
            .min(WIDTH * 256);
        let page = EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
            level: 0,
            x,
            y,
            width,
            height,
            kind: vec![crate::HydrologyKind::NoEvidence as u8; len],
            method: vec![0; len],
            water_model: None,
            river_segments: if x == 1 {
                vec![crate::RiverSegment {
                    start: [128 * 256, start_y],
                    end: [128 * 256, end_y],
                    width_units: 6 * 256,
                    station_start: i64::from(start_y),
                    station_end: i64::from(end_y),
                    source_delta: [0, WIDTH * 256],
                    source_station_delta: i64::from(WIDTH * 256),
                }]
            } else {
                vec![]
            },
        });
        pages.0.insert(page.key(), Arc::new(page));
        let page = EnvironmentPage::ModernLandCover(crate::ModernLandCoverPage {
            level: 0,
            x,
            y,
            width,
            height,
            worldcover_class: vec![0; len],
        });
        pages.0.insert(page.key(), Arc::new(page));
    }
    environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
        samples_per_axis: 129,
        page_samples: 64,
        world_cover_year: 2021,
        policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [3; 32],
        modern_land_cover_page_root: [4; 32],
        water_model: None,
    });
    let generator = bind(92, environment, pages);
    for y in [0, 31, 32, 125, 126, 127, 128, 129, 254, 255, 256] {
        let width = (122..=134)
            .filter(|x| generator.tile_at(TileCoord::new(*x, y)).unwrap().water == WaterKind::River)
            .count();
        assert!((5..=7).contains(&width), "page-edge channel width {width}");
        assert_eq!(
            generator.tile_at(TileCoord::new(128, y)).unwrap().water,
            WaterKind::River
        );
        for x in 122..=134 {
            let position = TileCoord::new(x, y);
            let first = generator.tile_at(position).expect("river tile");

            let chunk = generator.chunk(x / 32, y / 32).expect("chunk");
            assert_eq!(chunk.tiles[((y % 32) * 32 + x % 32) as usize], first);
            assert_eq!(generator.tile_at(position), Some(first));
        }
    }
}

#[test]
fn steep_crossings_keep_blocking_water_instead_of_impassable_ford_art() {
    let generator = crossing_with_relief(false, true);
    let mut river = 0;
    for y in 1..WIDTH - 1 {
        for x in 125..142 {
            let tile = generator.tile_at(TileCoord::new(x, y)).unwrap();
            if tile.water == WaterKind::River {
                river += 1;
            }
            if tile.material == GroundMaterial::Ford {
                assert!(tile.passable);
            }
        }
    }
    assert!(river > 1000);
}

#[path = "rivers/query_cache.rs"]
mod query_cache;

#[path = "rivers/shape.rs"]
mod shape;

#[test]
fn provider_rejects_out_of_margin_geometry_before_ocean_can_skip_river_queries() {
    let generator = crossing_with_geometry(
        true,
        false,
        vec![[0, 0, 67_108_864, 67_108_864, 768, 0, 1000]],
    );
    assert_eq!(
        generator.tile_at_with_cancel(TileCoord::new(10, 250), &|| false),
        Err(crate::EnvironmentPageError::Corrupt)
    );
}

#[path = "rivers/mouths.rs"]
mod mouths;

#[test]
fn ground_variety_preserves_textured_dirt_river_banks() {
    let generator = crossing();
    let tiles = composed(&generator);
    let mut banks = 0;
    let mut varied_land = std::collections::BTreeSet::new();
    for point in &tiles {
        if point.terrain.water != WaterKind::None || !point.terrain.passable {
            continue;
        }
        let position = point.tile;
        if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
            at(&tiles, position.x + dx, position.y + dy)
                .is_some_and(|neighbor| neighbor.terrain.water == WaterKind::River)
        }) {
            banks += 1;
            assert_eq!(
                point.terrain.material,
                GroundMaterial::Dirt,
                "bank {position:?}"
            );
            assert_eq!(
                generator
                    .landscape_point_with_cancel(position, &|| false)
                    .unwrap()
                    .unwrap()
                    .tile,
                *point
            );
        } else if matches!(
            point.terrain.material,
            GroundMaterial::WornGrass | GroundMaterial::GrassDirt | GroundMaterial::LushGrass
        ) {
            varied_land.insert(point.terrain.material as u8);
        }
    }
    assert!(banks > 400, "both banks remain a dirt band");
    assert!(
        varied_land.len() >= 2,
        "ground variety still appears away from banks"
    );
}

#[path = "rivers/cliffs.rs"]
mod cliffs;

#[path = "rivers/narrow.rs"]
mod narrow;
