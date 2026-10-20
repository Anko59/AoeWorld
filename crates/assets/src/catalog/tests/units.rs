use crate::catalog::units::{Pose, SPRITES};

#[test]
fn actor_animation_metadata_uses_dat_rows_and_individual_frame_durations() {
    for (pose, id, stride, millis) in [
        (Pose::MilitiaStand, 993, 6, 1000),
        (Pose::MilitiaWalk, 997, 12, 70),
        (Pose::MilitiaAttack, 987, 10, 100),
        (Pose::MilitiaDie, 990, 10, 100),
        (Pose::VillagerAttack, 1473, 15, 70),
        (Pose::VillagerDie, 1476, 15, 100),
        (Pose::HunterAttack, 1518, 15, 100),
        (Pose::Butcher, 1528, 15, 100),
        (Pose::HunterCarry, 1519, 15, 70),
        (Pose::ShepherdWork, 3682, 15, 100),
        (Pose::ShepherdDie, 3683, 15, 100),
        (Pose::HunterDie, 1522, 15, 100),
        (Pose::SheepStand, 3629, 9, 275),
        (Pose::DeerWalk, 348, 15, 50),
        (Pose::BoarAttack, 2555, 17, 95),
        (Pose::WolfAttack, 1629, 20, 85),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            (source.id, source.stride, source.millis),
            (id, stride, millis)
        );
        assert_eq!(source.frames(), u32::from(stride) * 5);
    }
}
