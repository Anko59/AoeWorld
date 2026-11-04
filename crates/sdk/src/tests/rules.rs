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
    assert_eq!(rules::building_requires(BuildingKind::TownCenter), None);
}

#[test]
fn road_rules_expose_the_dark_age_route_improvement() {
    use crate::{Age, BuildingKind};
    assert!(crate::rules::building_buildable(BuildingKind::Road));
    assert_eq!(crate::rules::building_cost(BuildingKind::Road).wood, 1);
    assert_eq!(crate::rules::building_build_seconds(BuildingKind::Road), 2);
    assert_eq!(
        crate::rules::building_required_age(BuildingKind::Road),
        Age::Dark
    );
}
