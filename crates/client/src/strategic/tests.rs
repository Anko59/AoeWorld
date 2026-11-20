use super::*;
use aoe_protocol::ConstructionState::{Complete, UnderConstruction};

#[test]
fn living_sites_and_foundations_show_in_the_far_view() {
    assert!(far_building_visible(1, Complete));
    assert!(far_building_visible(u16::MAX, Complete));
    assert!(!far_building_visible(0, Complete));
    // A foundation (hp 1) is a marker too; only the dead are hidden.
    assert!(far_building_visible(1, UnderConstruction));
    assert!(!far_building_visible(0, UnderConstruction));
}

#[test]
fn dominant_kind_is_exact_only_for_a_complete_army_report_and_ties_use_kind_order() {
    use super::armies::ArmyKinds;
    use aoe_protocol::UnitKind::{Archer, Knight, Militia};
    let mut army = ArmyKinds::default();
    for kind in [Knight, Militia, Archer, Knight, Archer] {
        army.add(kind);
    }
    assert_eq!(army.complete_dominant(5), Some(Archer));
    assert_eq!(army.complete_dominant(6), None);
    assert_eq!(army.complete_dominant(4), None);
    army.add(Knight);
    assert_eq!(army.complete_dominant(6), Some(Knight));
    assert_eq!(ArmyKinds::default().complete_dominant(0), None);
}
