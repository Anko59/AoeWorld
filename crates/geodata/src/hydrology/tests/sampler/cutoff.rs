//! The river discharge cutoff is a real-world constant: a map keeps the same
//! rivers over the same area at every map size.
use super::super::super::hydrology_sampling::VectorFeature;
use super::super::channels::{MAX_PAGE_SEGMENT_BYTES, minimum_discharge, segments};
use super::*;

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
fn discharge_cutoff_is_the_hydrorivers_floor_whatever_the_map_size() {
    // HydroRIVERS lists reaches from 0.1 m³/s; no map size raises the floor.
    assert_eq!(minimum_discharge(), 0.1);
}

#[test]
fn every_map_size_keeps_the_same_small_rivers_over_the_same_area() {
    let directory = TestDirectory::new();
    let discharges = [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 20.0, 100.0];
    let reaches = discharges
        .iter()
        .enumerate()
        .map(|(i, d)| (i as i64 + 1, *d, -2_000.0 + 400.0 * i as f64))
        .collect::<Vec<_>>();
    let features = river_features(&directory, &reaches);
    let bounds = [-15_000.0, -15_000.0, 15_000.0, 15_000.0];
    // The same real area: 30 km of landscape in 500 tiles of 60 m.
    let kept = segments(&features, 30_000.0, 500, bounds).expect("segments");
    let kept_northings = kept
        .iter()
        .map(|s| s.start[1])
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        kept_northings.len(),
        discharges.iter().filter(|d| **d >= 0.1).count()
    );
    assert!(
        kept.iter().any(|s| s.width_units == 3 * 256),
        "0.1 m³/s is kept"
    );
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
