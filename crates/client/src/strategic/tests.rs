use super::*;
use aoe_protocol::ConstructionState::{Complete, UnderConstruction};

#[test]
fn completed_sites_only_in_the_far_view() {
    assert!(far_building_visible(1, Complete));
    assert!(far_building_visible(u16::MAX, Complete));
    assert!(!far_building_visible(0, Complete));
    for hp in [0, 1, u16::MAX] {
        assert!(!far_building_visible(hp, UnderConstruction));
    }
}
