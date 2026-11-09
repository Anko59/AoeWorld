use super::super::routes::passes_json;
use aoe_simulation::{GameWorld, PASS_NAMES};

#[test]
fn the_routes_report_lists_every_pass_with_calls_and_visits() {
    let world = GameWorld::new(aoe_core::WorldConfig::new(64, 64, aoe_core::Seed(1)).unwrap());
    let world = world.unwrap();
    let report = passes_json(&world.pass_counts(), (3, 300));
    for name in PASS_NAMES {
        assert_eq!(report[name]["calls"], 0, "{name}");
        assert_eq!(report[name]["visited"], 0, "{name}");
    }
    assert_eq!(report["unit_frame"]["calls"], 3);
    assert_eq!(report["unit_frame"]["visited"], 300);
}
