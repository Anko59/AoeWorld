//! The river discharge cutoff rises with the tile size (compression): coarse
//! maps keep only major rivers, fine maps keep every HydroRIVERS reach.
use super::super::super::hydrology_sampling::VectorFeature;
use super::super::channels::{MAX_PAGE_SEGMENT_BYTES, base_discharge, minimum_discharge, segments};
use super::*;

pub(super) fn river_features(
    directory: &TestDirectory,
    reaches: &[(i64, f64, f64)],
) -> Vec<VectorFeature> {
    let path = directory.path("cutoff-rivers.gpkg");
    let dataset = vector_dataset(&path, "rivers", OGRwkbGeometryType::wkbLineString);
    {
        let layer = dataset.layer(0).expect("river layer");
        layer
            .create_defn_fields(&[
                ("HYRIV_ID", OGRFieldType::OFTInteger64),
                ("NEXT_DOWN", OGRFieldType::OFTInteger64),
                ("DIST_DN_KM", OGRFieldType::OFTReal),
                ("DIS_AV_CMS", OGRFieldType::OFTReal),
            ])
            .expect("river fields");
        for &(id, discharge, north) in reaches {
            let mut feature = Feature::new(layer.defn()).expect("river feature");
            feature.set_field_integer64(0, id).expect("id");
            feature.set_field_integer64(1, 0).expect("downstream");
            feature.set_field_double(2, 1.0).expect("distance");
            feature.set_field_double(3, discharge).expect("discharge");
            let line = line_wkt((-3_000.0, north), (3_000.0, north));
            feature
                .set_geometry(Geometry::from_wkt(&line).expect("line"))
                .expect("geometry");
            feature.create(&layer).expect("write reach");
        }
    }
    drop(dataset);
    let mut rivers = Dataset::open(path).expect("open rivers");
    let definition =
        crate::local_aeqd_definition(request().center_latitude_e7, request().center_longitude_e7);
    vector_features(&mut rivers, (-180.0, -90.0, 180.0, 90.0), &definition, true).expect("reaches")
}

#[test]
fn discharge_floor_scales_with_the_square_of_the_tile_size() {
    // Compression c means 2 m * c per tile. Up to the 60 m reference tile every
    // listed reach is kept; beyond it the floor grows with the tile size squared.
    let floor = |compression: f64| minimum_discharge(2.0 * compression);
    assert_eq!(base_discharge(), 0.1);
    assert_eq!(floor(1.0), 0.1);
    assert_eq!(floor(30.0), 0.1);
    assert!((floor(60.0) - 0.4).abs() < 1e-9);
    assert!((floor(120.0) - 1.6).abs() < 1e-9);
    assert!((floor(240.0) - 6.4).abs() < 1e-9);
    let mut previous = 0.0;
    for compression in [1.0, 30.0, 45.0, 90.0, 180.0, 240.0, 480.0] {
        assert!(floor(compression) >= previous, "monotonic in compression");
        previous = floor(compression);
    }
}

#[test]
fn coarse_maps_drop_small_rivers_and_fine_maps_keep_them() {
    let directory = TestDirectory::new();
    let discharges = [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 20.0, 100.0];
    let reaches = discharges
        .iter()
        .enumerate()
        .map(|(i, d)| (i as i64 + 1, *d, -2_000.0 + 400.0 * i as f64))
        .collect::<Vec<_>>();
    let bounds = [-15_000.0, -15_000.0, 15_000.0, 15_000.0];
    let kept_at = |compression: f64, tiles: u64| {
        let side = tiles as f64 * 2.0 * compression;
        let floor = minimum_discharge(side / tiles as f64);
        let surviving = reaches
            .iter()
            .copied()
            .filter(|&(_, discharge, _)| discharge >= floor)
            .collect::<Vec<_>>();
        let kept = river_features(&directory, &surviving);
        segments(&kept, side, tiles, bounds)
            .expect("segments")
            .iter()
            .map(|s| s.width_units)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    };
    // Distinct widths track distinct kept discharges: fewer rivers at 240.
    assert!(kept_at(30.0, 500) > kept_at(120.0, 125));
    assert!(kept_at(120.0, 125) > kept_at(240.0, 62));
    assert!(kept_at(240.0, 62) >= 1, "major rivers remain at 240");
    assert_eq!(kept_at(30.0, 500), kept_at(10.0, 1500));
}

#[test]
fn an_over_budget_page_keeps_the_highest_discharge_reaches_in_a_fixed_order() {
    let directory = TestDirectory::new();
    let total = aoe_map::MAX_RIVER_SEGMENTS_PER_PAGE + 76;
    // Discharge rises with the index and so does the northing, so the lowest
    // discharges are the first in file order: arbitrary truncation would keep them.
    let reaches = (0..total)
        .map(|i| {
            (
                i as i64 + 1,
                1.0 + i as f64 / 10.0,
                -11_000.0 + 20.0 * i as f64,
            )
        })
        .collect::<Vec<_>>();
    let features = river_features(&directory, &reaches);
    let bounds = [-15_000.0, -15_000.0, 15_000.0, 15_000.0];
    let kept = segments(&features, 30_000.0, 500, bounds)
        .expect("an over-budget page is capped, not rejected");
    assert!(kept.len() <= aoe_map::MAX_RIVER_SEGMENTS_PER_PAGE);
    assert!(
        kept.len() > 300,
        "the cap keeps a dense page, not an empty one"
    );
    let json = kept
        .iter()
        .map(|s| serde_json::to_vec(s).unwrap().len() + 1)
        .sum::<usize>();
    assert!(
        json <= MAX_PAGE_SEGMENT_BYTES,
        "serialized segments stay inside the page budget"
    );
    let units = 256.0 / 60.0;
    let mut index = kept
        .iter()
        .map(|s| ((26_000.0 - f64::from(s.start[1]) / units) / 20.0).round() as i64)
        .collect::<Vec<_>>();
    index.sort_unstable();
    assert_eq!(
        index,
        ((total - kept.len()) as i64..total as i64).collect::<Vec<_>>()
    );
    assert_eq!(
        segments(&features, 30_000.0, 500, bounds).expect("repeat"),
        kept,
        "the cap is deterministic"
    );
}
