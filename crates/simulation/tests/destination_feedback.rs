use aoe_simulation::destination_passable;

#[test]
fn destination_feedback_uses_the_same_ground_and_occupancy_rule_as_movement() {
    for ground in [false, true] {
        for resource in [false, true] {
            for building in [false, true] {
                assert_eq!(
                    destination_passable(ground, resource, building),
                    ground && !resource && !building
                );
            }
        }
    }
}
