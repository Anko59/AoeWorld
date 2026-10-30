use crate::catalog::units::{Pose, SPRITES};

#[test]
fn siege_stone_keeps_all_thirty_native_de_rotation_cells_at_sixty_fps() {
    let source = SPRITES[Pose::Stone as usize];
    assert_eq!(source.manifest_source(), "de/p_mangonel_x1.sld");
    assert_eq!((source.frames(), source.rows(), source.stride), (30, 1, 30));
    assert_eq!((source.sample_step(), source.selected_frames()), (1, 30));
    assert!((source.frame_millis() - 16.66666753590107).abs() < 0.000001);
    for phase in 0..30 {
        assert_eq!(source.source_frame(0, phase), phase);
    }
}

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

/// Unpacks a projectile `facing | phase << 3` into the native SLP direction
/// block and pitch cell it draws, and whether it is mirrored.
fn cell(pose: Pose, packed: u8) -> (usize, usize, bool) {
    let source = SPRITES[pose as usize];
    let facing = usize::from(packed & 7);
    let row = [0, 1, 2, 3, 4, 3, 2, 1][facing];
    let frame = source.source_frame(row, usize::from(packed >> 3));
    let per = if pose == Pose::Arrow { 11 } else { 10 };
    (frame / per, frame % per, (1..=3).contains(&facing))
}

#[test]
fn projectile_cells_cover_every_direction_within_twenty_three_degrees() {
    use crate::catalog::units::projectile_facing;
    for pose in [Pose::Arrow, Pose::Javelin] {
        let source = SPRITES[pose as usize];
        assert_eq!(source.selected_frames(), 5);
        let per = if pose == Pose::Arrow { 11 } else { 10 };
        for degrees in 0..360 {
            // Screen direction of the flight, measured from south through west.
            let angle = f64::from(degrees).to_radians();
            let (east, south) = (angle.sin(), angle.cos());
            let (block, pitch, mirrored) = cell(pose, projectile_facing(east, south));
            assert!(!mirrored || east > 0.0);
            assert!(block * per + pitch < source.frames() as usize);
            let drawn = (block as f64 * 11.25).min(180.0);
            let wanted = if degrees <= 180 {
                degrees
            } else {
                360 - degrees
            };
            assert!(
                (drawn - f64::from(wanted)).abs() <= 22.5 + 1e-9,
                "{pose:?} {degrees}: drew {drawn}"
            );
        }
    }
}

#[test]
fn projectile_facing_follows_the_cardinal_screen_directions() {
    use crate::catalog::units::projectile_facing as facing;
    // West is the unmirrored native row, east its mirror, level at cell 5.
    assert_eq!(cell(Pose::Arrow, facing(-1.0, 0.0)), (8, 5, false));
    assert_eq!(cell(Pose::Arrow, facing(1.0, 0.0)), (8, 5, true));
    assert_eq!(facing(1.0, 0.0) & 7, 2);
    assert_eq!(facing(-1.0, 0.0) & 7, 6);
    assert_eq!(facing(0.0, 1.0) & 7, 0);
    assert_eq!(facing(0.0, -1.0) & 7, 4);
    assert_eq!(cell(Pose::Javelin, facing(0.0, -1.0)).0, 16);
    assert_eq!(cell(Pose::Arrow, facing(0.0, -1.0)).0, 15);
}

#[test]
fn every_shooting_attack_releases_inside_its_animation() {
    use crate::catalog::units::release_frame;
    for pose in [
        Pose::ArcherAttack,
        Pose::SkirmisherAttack,
        Pose::CrossbowmanAttack,
        Pose::EliteSkirmisherAttack,
    ] {
        let release = release_frame(pose).unwrap();
        assert!(release > 0 && release < SPRITES[pose as usize].stride);
    }
    // Melee and siege poses release nothing mid-animation.
    assert_eq!(release_frame(Pose::KnightAttack), None);
    assert_eq!(release_frame(Pose::MangonelAttack), None);
}

#[test]
fn general_uses_distinct_full_paladin_animations() {
    for (pose, name, stride) in [
        (Pose::GeneralStand, "idle", 45),
        (Pose::GeneralWalk, "walk", 30),
        (Pose::GeneralAttack, "attack", 45),
        (Pose::GeneralDie, "death", 45),
        (Pose::GeneralDecay, "decay", 30),
    ] {
        let s = SPRITES[pose as usize];
        assert_eq!(
            s.manifest_source(),
            format!("de/u_cav_hero_frankish_paladin_{name}A_x1.sld")
        );
        assert_eq!(s.selected_frames(), stride * 16);
        assert_eq!(s.rows(), 16);
        for row in 0..16 {
            for phase in 0..stride {
                assert_eq!(s.source_frame(row, phase), row * stride + phase);
                assert!(s.source_frame(row, phase) < s.frames() as usize);
            }
        }
    }
}
