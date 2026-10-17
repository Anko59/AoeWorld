//! The river discharge cutoff is a real-world constant: a map keeps the same
//! rivers over the same area at every map size.
use super::super::super::hydrology_sampling::VectorFeature;
use super::super::channels::{minimum_discharge, segments};
use super::*;

/// Evidence-cell width in metres of the four skirmish sizes (30, 120, 480 and
/// 1,920 km per side) at the landscape axis of 1,024 samples.
const SIZES: [(f64, &str); 4] = [
    (30_000.0, "Tiny"),
    (120_000.0, "Small"),
    (480_000.0, "Large"),
    (1_920_000.0, "Giant"),
];
const LANDSCAPE_AXIS: f64 = 1024.0;

fn river_features(directory: &TestDirectory, reaches: &[(i64, f64, f64)]) -> Vec<VectorFeature> {
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
fn discharge_cutoff_is_identical_at_every_map_size() {
    let tiny = minimum_discharge(SIZES[0].0 / LANDSCAPE_AXIS);
    for (side, name) in SIZES {
        assert_eq!(
            minimum_discharge(side / LANDSCAPE_AXIS),
            tiny,
            "{name} must use the same real-world cutoff as Tiny"
        );
    }
}

#[test]
fn large_map_keeps_the_small_rivers_a_tiny_map_keeps_over_the_same_area() {
    let directory = TestDirectory::new();
    let discharges = [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 20.0, 100.0];
    let reaches = discharges
        .iter()
        .enumerate()
        .map(|(i, d)| (i as i64 + 1, *d, -2_000.0 + 400.0 * i as f64))
        .collect::<Vec<_>>();
    let features = river_features(&directory, &reaches);
    let bounds = [-15_000.0, -15_000.0, 15_000.0, 15_000.0];
    // The same 30 km of real landscape and 60 m tiles, whatever the sampling spacing.
    let tiny = segments(&features, 30_000.0, 500, bounds, 30_000.0 / LANDSCAPE_AXIS)
        .expect("Tiny segments");
    let large = segments(&features, 30_000.0, 500, bounds, 480_000.0 / LANDSCAPE_AXIS)
        .expect("Large segments");
    assert!(tiny.len() >= 5, "the fixture keeps its larger rivers");
    assert_eq!(large, tiny, "Large keeps exactly the rivers Tiny keeps");
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
    let kept = segments(&features, 30_000.0, 500, bounds, 480_000.0 / LANDSCAPE_AXIS)
        .expect("an over-budget page is capped, not rejected");
    assert_eq!(kept.len(), aoe_map::MAX_RIVER_SEGMENTS_PER_PAGE);
    let units = 256.0 / 60.0;
    let mut north = kept
        .iter()
        .map(|s| ((26_000.0 - f64::from(s.start[1]) / units) / 20.0).round() as i64)
        .collect::<Vec<_>>();
    north.sort_unstable();
    assert_eq!(north, (76..total as i64).collect::<Vec<_>>());
}
