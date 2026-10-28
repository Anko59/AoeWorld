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

#[test]
fn castle_actor_selection_keeps_native_sources_inside_the_existing_budget() {
    for (pose, id) in [
        (Pose::KnightStand, 669),
        (Pose::MonkWalk, 779),
        (Pose::MangonelAttack, 716),
        (Pose::BatteringRamDie, 176),
    ] {
        let s = SPRITES[pose as usize];
        assert_eq!(s.id, id);
        assert!(s.selected_frames() <= 10);
    }
    assert_eq!(SPRITES[Pose::KnightWalk as usize].millis, 110);
}

#[test]
fn navy_actor_sources_sample_eight_facings_from_original_sixteen_direction_art() {
    for (pose, name) in [
        (Pose::Galley, "galley"),
        (Pose::WarGalley, "war_galley"),
        (Pose::FireShip, "fire_ship"),
        (Pose::DemolitionShip, "demo_ship"),
        (Pose::TransportShip, "transport_ship"),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(source.frames(), 16);
        assert_eq!(source.selected_frames(), 5);
        assert_eq!(source.manifest_source(), format!("de/u_shp_{name}_x1.sld"));
        for row in 0..5 {
            // Native DE frame zero faces screen east; the existing mirrored
            // actor rows begin south, then SW, west, NW and north.
            assert_eq!(source.source_frame(row, 0), 4 + row * 2);
        }
    }
}

#[test]
fn vassal_uses_king_poses_with_native_timing() {
    // Trial DAT confirms these King identities/timings but its DRS omits all
    // four bodies; the owner's licensed DE King is the explicit pixel source.
    for (pose, name, id, stride, millis, frames) in [
        (Pose::KingStand, "idle", 1767, 6, 200, 961),
        (Pose::KingWalk, "walk", 1771, 10, 70, 481),
        (Pose::KingDie, "death", 1764, 10, 100, 481),
        (Pose::KingDecay, "decay", 1768, 5, 6000, 481),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            (source.id, source.stride, source.millis, source.frames()),
            (id, stride, millis, frames)
        );
        assert_eq!(
            source.manifest_source(),
            format!("de/u_king_west_{name}A_x1.sld")
        );
        for row in 0..5 {
            for phase in (0..source.stride as usize).step_by(source.sample_step() as usize) {
                assert!(source.source_frame(row, phase) < frames as usize);
            }
        }
    }
    assert!(crate::dat::table::DARK_AGE.iter().any(|(_, id)| *id == 434));
}

#[test]
fn projectile_rows_use_the_native_flight_pitch_for_each_screen_direction() {
    // The 32-angle DAT graphics store sixteen rows, each with pitch frames.
    // Frame zero is steeply pitched; these samples match level projected flight.
    for (pose, expected) in [
        (Pose::Arrow, [6, 49, 93, 137, 175]),
        (Pose::Javelin, [5, 45, 84, 125, 169]),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(source.selected_frames(), 5);
        assert_eq!(
            (0..5)
                .map(|row| source.source_frame(row, 0))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            expected
                .into_iter()
                .all(|frame| frame < source.frames() as usize)
        );
    }
}
