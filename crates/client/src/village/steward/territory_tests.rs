use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn town_center(id: u32, x: f64, active: bool) -> TownCenter {
    TownCenter {
        id: EntityId(id),
        center: [x, 90.0],
        active,
    }
}
fn nearest(point: [f64; 2], all: &[TownCenter]) -> Option<EntityId> {
    all.iter()
        .filter(|c| c.active)
        .min_by(|a, b| {
            let d = |c: &TownCenter| (c.center[0] - point[0]).hypot(c.center[1] - point[1]);
            d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
        })
        .map(|c| c.id)
}

#[wasm_bindgen_test]
fn a_lone_town_center_draws_its_full_circle() {
    let a = town_center(1, 90.0, true);
    for p in outline(&a, &[a], 100.0, 64) {
        assert!(((p[0] - 90.0).hypot(p[1] - 90.0) - 100.0).abs() < 1e-9);
    }
}

#[wasm_bindgen_test]
fn overlapping_town_centers_meet_at_the_bisector_with_a_double_line() {
    let (a, b) = (town_center(1, 90.0, true), town_center(2, 150.0, true));
    let all = [a, b];
    let (left, right) = (outline(&a, &all, 100.0, 128), outline(&b, &all, 100.0, 128));
    // The bisector is x = 120; each outline stops short of it by the gap.
    let a_max = left.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
    let b_min = right.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
    assert!((a_max - (120.0 - BORDER_HALF_GAP)).abs() < 1e-6, "{a_max}");
    assert!((b_min - (120.0 + BORDER_HALF_GAP)).abs() < 1e-6, "{b_min}");
    assert!(b_min - a_max > 0.5, "two distinct lines");
    // Every vertex lies in its own cell, so the cells never overlap.
    assert!(left.iter().all(|p| nearest(*p, &all) == Some(a.id)));
    assert!(right.iter().all(|p| nearest(*p, &all) == Some(b.id)));
}

#[wasm_bindgen_test]
fn town_centers_beyond_two_radii_do_not_interact_and_inactive_ones_claim_nothing() {
    let (a, far) = (town_center(1, 90.0, true), town_center(2, 300.0, true));
    assert_eq!(
        outline(&a, &[a, far], 100.0, 32),
        outline(&a, &[a], 100.0, 32)
    );
    let idle = town_center(3, 150.0, false);
    assert_eq!(
        outline(&a, &[a, idle], 100.0, 32),
        outline(&a, &[a], 100.0, 32)
    );
    let full = outline(&idle, &[a, idle], 100.0, 32);
    assert!(
        full.iter()
            .all(|p| ((p[0] - 150.0).hypot(p[1] - 90.0) - 100.0).abs() < 1e-9)
    );
}
