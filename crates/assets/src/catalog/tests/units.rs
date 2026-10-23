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
        (Pose::FishingShipStand, 444, 1, 200),
        (Pose::FishingShipWalk, 449, 1, 200),
        (Pose::FishingShipWork, 444, 1, 200),
        (Pose::FisherWork, 3575, 28, 200),
        (Pose::ArcherStand, 8, 10, 200),
        (Pose::ArcherWalk, 12, 10, 70),
        (Pose::ArcherAttack, 2, 10, 70),
        (Pose::ArcherDie, 5, 10, 100),
        (Pose::ArcherDecay, 9, 5, 6000),
        (Pose::SkirmisherStand, 1650, 8, 1000),
        (Pose::SkirmisherWalk, 1654, 10, 70),
        (Pose::SkirmisherAttack, 1644, 12, 100),
        (Pose::SkirmisherDie, 1647, 10, 100),
        (Pose::SkirmisherDecay, 1651, 5, 6000),
        (Pose::SpearmanStand, 873, 9, 120),
        (Pose::SpearmanWalk, 877, 10, 70),
        (Pose::SpearmanAttack, 867, 10, 100),
        (Pose::SpearmanDie, 870, 10, 100),
        (Pose::SpearmanDecay, 874, 5, 6000),
        (Pose::ManAtArmsStand, 1044, 11, 250),
        (Pose::ManAtArmsWalk, 1048, 11, 85),
        (Pose::ManAtArmsAttack, 1038, 11, 160),
        (Pose::ManAtArmsDie, 1041, 11, 100),
        (Pose::ManAtArmsDecay, 1045, 5, 6000),
        (Pose::ScoutStand, 2085, 10, 200),
        (Pose::ScoutWalk, 2089, 10, 110),
        // The farmer's own tilling row (VMFAR_TN), not the forager's.
        (Pose::FarmerWork, 1512, 15, 133),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            (source.id, source.stride, source.millis),
            (id, stride, millis)
        );
        assert_eq!(source.frames(), u32::from(stride) * 5);
    }
}
