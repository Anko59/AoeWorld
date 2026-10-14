use super::*;

#[test]
fn irregular_polygon_samples_keep_notch_and_island() {
    use super::super::super::hydrology_sampling::VectorFeature;
    let geometry = Geometry::from_wkt("POLYGON ((1 1, 31 1, 31 31, 20 31, 20 23, 14 23, 14 31, 1 31, 1 1), (7 7, 7 12, 12 12, 12 7, 7 7))").unwrap();
    let expected = geometry.clone();
    let samples = super::super::lakes::rasterize_lakes(
        vec![VectorFeature {
            geometry,
            kind: HydrologyKind::Lake,
            river_reach: None,
        }],
        32,
        32,
        0.0,
        32.0,
        1.0,
    )
    .unwrap();
    for y in 0..32 {
        for x in 0..32 {
            let point =
                Geometry::from_wkt(&format!("POINT ({} {})", x as f64 + 0.5, 31.5 - y as f64))
                    .unwrap();
            assert_eq!(
                samples[y * 32 + x].is_some(),
                expected.contains(&point),
                "sample {x},{y}"
            );
        }
    }
    for (x, y) in [(16, 4), (9, 22)] {
        for dy in -1..=1 {
            for dx in -1..=1 {
                assert!(samples[((y + dy) * 32 + x + dx) as usize].is_none());
            }
        }
    }
}
