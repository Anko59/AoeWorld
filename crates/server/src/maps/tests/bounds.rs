//! Height bounds must come out of the opening scan, which has just decoded
//! every page: no page is read a second time for them.
use super::super::page_residency;
use super::stored::{json, page_path, publish, served_bounds, started};
use crate::{AppState, map_store::PageResidency};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, EnvironmentalProvenance, FieldPyramid,
    HydrologyEvidenceIndex, HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind,
    HydrologyWaterModelIndex, HydrologyWaterModelPage, HydrologyWaterPolicy, MAP_SCHEMA_VERSION,
    MapPackage, MapRequest, ModernLandCoverPage, PageLayer, PreparedEnvironment,
    ProjectionMetadata, PyramidLevel, Ratio, WORLD_COVER_OBSERVATION_YEAR, WaterFlowDirection,
    WaterModelProvenance, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
    ordered_page_root,
};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};

const SIDE: u16 = ENVIRONMENT_PAGE_SAMPLES as u16;

/// One synthetic package: heights by (level, x, y), surfaces by (x, y).
struct Case {
    name: &'static str,
    compression: (u32, u32),
    elevation_axis: u16,
    height: fn(u8, u16, u16) -> i32,
    evidence: Option<Evidence>,
    /// Worked out by hand, not by a second walk that could be wrong like the scan.
    expected: (i64, i64),
}

struct Evidence {
    axis: u16,
    modeled: bool,
    surface: fn(u16, u16) -> Option<i32>,
}

fn sample(heights: [i32; 4], x: u16, y: u16) -> i32 {
    heights[usize::from(y * 2 + x)]
}

fn relief(_: u8, x: u16, y: u16) -> i32 {
    sample([100, 200, 300, 400], x, y)
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "levels truncate toward zero, below it too",
            compression: (1, 1),
            elevation_axis: 2,
            height: |level, x, y| match level {
                0 => sample([-199, -101, -100, -99], x, y),
                _ => -150,
            },
            evidence: None,
            expected: (-1, 0),
        },
        Case {
            name: "compression divides before the level is taken",
            compression: (30, 1),
            elevation_axis: 2,
            height: |level, x, y| match level {
                0 => sample([-2_999, 2_999, 3_000, 89_999], x, y),
                _ => 0,
            },
            evidence: None,
            expected: (0, 29),
        },
        Case {
            name: "a fractional compression multiplies by its denominator",
            compression: (7, 2),
            elevation_axis: 2,
            height: |level, x, y| match level {
                0 => sample([-1_000, 0, 350, 1_049], x, y),
                _ => 0,
            },
            evidence: None,
            expected: (-2, 2),
        },
        Case {
            name: "levels clamp to the range of a game height",
            compression: (1, 1),
            elevation_axis: 2,
            height: |level, x, y| match level {
                0 => sample([i32::MIN, 0, 0, i32::MAX], x, y),
                _ => 0,
            },
            evidence: None,
            expected: (-32_768, 32_767),
        },
        Case {
            name: "overview levels never widen the range",
            compression: (1, 1),
            elevation_axis: 3,
            height: |level, _, _| match level {
                0 => 500,
                1 => -900_000,
                _ => 900_000,
            },
            evidence: None,
            expected: (5, 5),
        },
        Case {
            name: "every page of an uneven grid counts, the partial ones too",
            compression: (1, 1),
            elevation_axis: 130,
            height: |level, x, y| match (level, x, y) {
                (0, 129, 129) => 777_700,
                (0, 0, 129) => -333_300,
                (0, ..) => 100,
                _ => 2_000_000,
            },
            evidence: None,
            expected: (-3_333, 7_777),
        },
        Case {
            name: "modeled water widens both ends on an axis of its own",
            compression: (1, 1),
            elevation_axis: 130,
            height: |_, _, _| 10_000,
            evidence: Some(Evidence {
                axis: 70,
                modeled: true,
                surface: |x, y| match (x, y) {
                    (69, 69) => Some(450_000),
                    (0, 69) => Some(-70_050),
                    _ => None,
                },
            }),
            expected: (-700, 4_500),
        },
        Case {
            name: "modeled water within the relief changes nothing",
            compression: (1, 1),
            elevation_axis: 2,
            height: |_, x, y| sample([-1_000, 5_000, 0, 0], x, y),
            evidence: Some(Evidence {
                axis: 70,
                modeled: true,
                surface: |_, _| Some(100),
            }),
            expected: (-10, 50),
        },
        Case {
            name: "modeled water is compressed like the ground",
            compression: (30, 1),
            elevation_axis: 2,
            height: |_, _, _| 0,
            evidence: Some(Evidence {
                axis: 2,
                modeled: true,
                surface: |x, _| Some(if x == 0 { -2_999 } else { 5_999 }),
            }),
            expected: (0, 1),
        },
        Case {
            name: "evidence without a model has no surface",
            compression: (1, 1),
            elevation_axis: 2,
            height: relief,
            evidence: Some(Evidence {
                axis: 70,
                modeled: false,
                surface: |_, _| None,
            }),
            expected: (1, 4),
        },
        Case {
            name: "a model of dry land has no surface",
            compression: (1, 1),
            elevation_axis: 2,
            height: relief,
            evidence: Some(Evidence {
                axis: 70,
                modeled: true,
                surface: |_, _| None,
            }),
            expected: (1, 4),
        },
    ]
}

fn span(axis: u16, page: u16) -> u8 {
    (axis - page * SIDE).min(SIDE) as u8
}

/// The samples of one page in storage order.
fn samples(axis: u16, x: u16, y: u16) -> impl Iterator<Item = (u16, u16)> {
    (0..u16::from(span(axis, y))).flat_map(move |dy| {
        (0..u16::from(span(axis, x))).map(move |dx| (x * SIDE + dx, y * SIDE + dy))
    })
}

fn grid(axis: u16) -> impl Iterator<Item = (u16, u16)> {
    let count = axis.div_ceil(SIDE);
    (0..count).flat_map(move |y| (0..count).map(move |x| (x, y)))
}

fn elevation(axis: u16, height: fn(u8, u16, u16) -> i32) -> (FieldPyramid, Vec<ElevationPage>) {
    let (mut levels, mut pages) = (Vec::new(), Vec::new());
    let (mut level, mut remaining) = (0, axis);
    loop {
        let of_level: Vec<ElevationPage> = grid(remaining)
            .map(|(x, y)| ElevationPage {
                level,
                x,
                y,
                width: span(remaining, x),
                height: span(remaining, y),
                geographic_height_centimeters: samples(remaining, x, y)
                    .map(|(x, y)| height(level, x, y))
                    .collect(),
            })
            .collect();
        levels.push(PyramidLevel {
            samples_per_axis: remaining,
            ordered_page_root: ordered_page_root(&of_level).expect("elevation root"),
        });
        pages.extend(of_level);
        if remaining == 1 {
            return (FieldPyramid { levels }, pages);
        }
        remaining = remaining.div_ceil(2);
        level += 1;
    }
}

fn hydrology_page(evidence: &Evidence, x: u16, y: u16) -> HydrologyEvidencePage {
    let surfaces: Vec<Option<i32>> = samples(evidence.axis, x, y)
        .map(|(x, y)| (evidence.surface)(x, y))
        .collect();
    let pick = |wet: u8, dry: u8| -> Vec<u8> {
        surfaces
            .iter()
            .map(|surface| if surface.is_some() { wet } else { dry })
            .collect()
    };
    let kind = pick(HydrologyKind::Lake as u8, HydrologyKind::Land as u8);
    HydrologyEvidencePage {
        level: 0,
        x,
        y,
        width: span(evidence.axis, x),
        height: span(evidence.axis, y),
        kind: kind.clone(),
        method: pick(
            HydrologyEvidenceMethod::HydroLakesExtent as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
        ),
        river_segments: Vec::new(),
        water_model: evidence.modeled.then(|| HydrologyWaterModelPage {
            kind,
            flow_direction: vec![WaterFlowDirection::Unknown as u8; surfaces.len()],
            provenance: pick(
                WaterModelProvenance::ModelledLakeSurface as u8,
                WaterModelProvenance::EvidenceOnly as u8,
            ),
            surface_level_centimeters: surfaces.clone(),
        }),
    }
}

fn evidence_index(
    request: MapRequest,
    evidence: &Evidence,
) -> (HydrologyEvidenceIndex, Vec<EnvironmentPage>) {
    let axis = evidence.axis;
    let hydrology: Vec<HydrologyEvidencePage> = grid(axis)
        .map(|(x, y)| hydrology_page(evidence, x, y))
        .collect();
    let land_cover: Vec<ModernLandCoverPage> = grid(axis)
        .map(|(x, y)| ModernLandCoverPage {
            level: 0,
            x,
            y,
            width: span(axis, x),
            height: span(axis, y),
            worldcover_class: vec![40; samples(axis, x, y).count()],
        })
        .collect();
    let index = HydrologyEvidenceIndex {
        samples_per_axis: axis,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(&hydrology).expect("hydrology root"),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(&land_cover)
            .expect("land-cover root"),
        water_model: evidence.modeled.then(|| HydrologyWaterModelIndex {
            model_version: aoe_map::HYDROLOGY_WATER_MODEL_VERSION,
            samples_per_axis: axis,
            target_year_ce: aoe_map::WATER_CORRECTION_TARGET_YEAR_CE,
            correction_document: aoe_map::WaterCorrectionDocument::empty(request, axis)
                .expect("correction document"),
        }),
    };
    let pages = hydrology
        .into_iter()
        .map(EnvironmentPage::HydrologyEvidence)
        .chain(land_cover.into_iter().map(EnvironmentPage::ModernLandCover))
        .collect();
    (index, pages)
}

impl Case {
    fn build(&self) -> (MapPackage, Vec<EnvironmentPage>) {
        let (numerator, denominator) = self.compression;
        let request = MapRequest {
            compression: Ratio::new(numerator, denominator).expect("compression"),
            ..MapRequest::default()
        };
        let (pyramid, heights) = elevation(self.elevation_axis, self.height);
        let mut pages: Vec<EnvironmentPage> = heights
            .into_iter()
            .map(EnvironmentPage::Elevation)
            .collect();
        let hydrology_evidence = self.evidence.as_ref().map(|evidence| {
            let (index, typed) = evidence_index(request, evidence);
            pages.extend(typed);
            index
        });
        let package = MapPackage::with_prepared_environment(
            MAP_SCHEMA_VERSION,
            request,
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            PreparedEnvironment {
                samples_per_axis: self.elevation_axis,
                geographic_millimeters_per_sample: 1_000,
                page_samples: ENVIRONMENT_PAGE_SAMPLES,
                elevation: pyramid,
                hydrology_evidence,
                ..PreparedEnvironment::default()
            },
        )
        .expect("synthetic package");
        (package, pages)
    }
}

async fn open(state: &AppState, package: &MapPackage) -> Arc<PageResidency> {
    page_residency(state, package, Arc::new(AtomicBool::new(false)))
        .await
        .expect("open")
        .expect("a prepared package has pages")
}

/// Pages read since the package was opened, and pages its cache holds.
fn read_since_open(provider: &PageResidency) -> (u64, usize) {
    (provider.verified_page_loads(), provider.resident_pages())
}

#[tokio::test]
async fn bounds_after_open_are_those_worked_out_and_read_no_page_again() {
    let directory = tempfile::tempdir().expect("package directory");
    let built: Vec<_> = cases()
        .into_iter()
        .map(|case| {
            let (package, pages) = case.build();
            publish(directory.path(), &package, &pages);
            (case, package)
        })
        .collect();
    let state = started(directory.path());
    for (case, package) in &built {
        let name = case.name;
        let provider = open(&state, package).await;
        assert_eq!(read_since_open(&provider), (0, 0), "{name}: just opened");
        for asked in ["first", "again"] {
            let served = served_bounds(&state, package).await;
            assert_eq!(served, Ok(case.expected), "{name}: asked {asked}");
            assert_eq!(
                read_since_open(&provider),
                (0, 0),
                "{name}: asked {asked}, (pages read, pages cached) since the opening"
            );
        }
    }
    // The registry keeps two packages, so most of these open again: asking for
    // the bounds is then the opening, and still reads each page only once.
    for (case, package) in built.iter().rev() {
        let served = served_bounds(&state, package).await;
        assert_eq!(served, Ok(case.expected), "{}: reopened", case.name);
        let provider = open(&state, package).await;
        assert_eq!(read_since_open(&provider), (0, 0), "{}", case.name);
    }
}

/// Bounds read through the cache would fill its 128 pages and push out those a
/// running game is using.
#[tokio::test]
async fn bounds_leave_the_pages_a_game_is_using_in_the_cache() {
    let (package, pages) = Case {
        name: "256 level-zero pages",
        compression: (1, 1),
        elevation_axis: 1_024,
        height: |_, x, _| i32::from(x) * 100,
        evidence: None,
        expected: (0, 1_023),
    }
    .build();
    let directory = tempfile::tempdir().expect("package directory");
    publish(directory.path(), &package, &pages);
    let state = started(directory.path());
    let provider = open(&state, &package).await;
    let in_use = [(0, 0), (7, 7), (15, 15)].map(|(x, y)| EnvironmentPageKey {
        layer: PageLayer::Elevation,
        level: 0,
        x,
        y,
    });
    let held = in_use.map(|key| provider.page(key, &|| false).expect("page in use"));
    assert_eq!(read_since_open(&provider), (3, 3));
    assert_eq!(served_bounds(&state, &package).await, Ok((0, 1_023)));
    for (key, page) in in_use.into_iter().zip(held) {
        let again = provider.page(key, &|| false).expect("page still in use");
        assert!(Arc::ptr_eq(&again, &page), "{key:?} was read again");
    }
    assert_eq!(read_since_open(&provider), (3, 3));
}

/// A page damaged after the opening does not fail the bounds: they are those of
/// the pages as they were opened. Whoever loads that page is still refused.
#[tokio::test]
async fn bounds_outlive_a_page_damaged_after_the_opening_and_loading_it_fails() {
    let (package, pages) = Case {
        name: "opened whole, damaged afterwards",
        compression: (1, 1),
        elevation_axis: 2,
        height: relief,
        evidence: None,
        expected: (1, 4),
    }
    .build();
    let directory = tempfile::tempdir().expect("package directory");
    publish(directory.path(), &package, &pages);
    let state = started(directory.path());
    let provider = open(&state, &package).await;
    let key = pages[0].key();
    let path = page_path(directory.path(), &package, key);
    let higher = EnvironmentPage::Elevation(ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![999_900; 4],
    });
    fs::write(&path, json(&higher)).expect("damaged page");
    assert_eq!(served_bounds(&state, &package).await, Ok((1, 4)));
    let load = || provider.page(key, &|| false).expect_err("refused page");
    assert_eq!(load(), EnvironmentPageError::Corrupt);
    fs::remove_file(&path).expect("missing page");
    assert_eq!(served_bounds(&state, &package).await, Ok((1, 4)));
    assert_eq!(load(), EnvironmentPageError::Missing);
    assert_eq!(read_since_open(&provider), (0, 0));
}
