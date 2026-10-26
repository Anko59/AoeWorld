use crate::{Age, BuildingKind, rules};

#[test]
fn town_center_construction_rules_are_exposed_to_bots() {
    assert!(rules::building_buildable(BuildingKind::TownCenter));
    assert_eq!(
        rules::building_required_age(BuildingKind::TownCenter),
        Age::Castle
    );
    let cost = rules::building_cost(BuildingKind::TownCenter);
    assert_eq!(
        (cost.food, cost.wood, cost.gold, cost.stone),
        (0, 275, 0, 100)
    );
    assert_eq!(rules::building_build_seconds(BuildingKind::TownCenter), 150);
    assert_eq!(rules::building_population(BuildingKind::TownCenter), 5);
    assert_eq!(rules::building_requires(BuildingKind::TownCenter), None);
}
