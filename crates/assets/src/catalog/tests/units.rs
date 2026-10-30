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
    for (pose, id, stride, millis, cells) in [
        (Pose::VillagerAttack, 201278, 60, 25, 961),
        (Pose::VillagerDie, 201281, 30, 50, 481),
        (Pose::HunterAttack, 201317, 30, 50, 481),
        (Pose::Butcher, 201602, 30, 50, 481),
        (Pose::HunterCarry, 201601, 60, 15, 961),
        (Pose::ShepherdWork, 203873, 30, 50, 481),
        (Pose::ShepherdDie, 203190, 60, 25, 961),
        (Pose::HunterDie, 201320, 60, 25, 961),
        (Pose::SheepStand, 203178, 60, 50, 960),
        (Pose::DeerWalk, 200768, 30, 25, 480),
        (Pose::BoarAttack, 202453, 45, 36, 721),
        (Pose::WolfAttack, 201382, 60, 28, 961),
        (Pose::FishingShipStand, 200803, 1, 200, 16),
        (Pose::FishingShipWalk, 200807, 1, 100, 16),
        (Pose::FishingShipWork, 201594, 1, 200, 16),
        (Pose::FisherWork, 203154, 60, 93, 961),
        (Pose::FarmerWork, 201600, 60, 50, 961),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            (source.id, source.stride, source.millis, source.frames()),
            (id, stride, millis, cells)
        );
        assert_eq!(source.rows(), 16);
    }
}

#[test]
fn castle_actor_selection_keeps_all_native_direction_blocks() {
    for (pose, id) in [
        (Pose::KnightStand, 200933),
        (Pose::MonkWalk, 201002),
        (Pose::MangonelAttack, 200979),
        (Pose::BatteringRamDie, 200683),
    ] {
        let s = SPRITES[pose as usize];
        assert_eq!(s.id, id);
        assert_eq!(s.sample_step(), 1);
        assert_eq!(s.selected_frames(), usize::from(s.stride) * 16);
    }
    assert!(
        (SPRITES[Pose::KnightWalk as usize].frame_millis() - 36.666665226221085).abs() < 0.00001
    );
}

#[test]
fn navy_actor_sources_keep_every_native_direction_and_sail_phase() {
    for (pose, id, stride, cells) in [
        (Pose::Galley, 204052, 30, 480),
        (Pose::WarGalley, 204013, 30, 480),
        (Pose::FireShip, 204182, 30, 480),
        (Pose::DemolitionShip, 204039, 30, 480),
        (Pose::TransportShip, 204093, 30, 480),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            (source.id, source.stride, source.frames()),
            (id, stride, cells)
        );
        assert_eq!(source.rows(), 16);
        assert_eq!(source.sample_step(), 1);
        for row in 0..16 {
            for phase in 0..stride as usize {
                assert_eq!(
                    source.source_frame(row, phase),
                    row * stride as usize + phase
                );
            }
        }
    }
}

#[test]
fn vassal_uses_king_poses_with_native_timing() {
    // The DE King contributes all four complete sixteen-direction timelines.
    for (pose, name, id, stride, millis, frames) in [
        (Pose::KingStand, "dat_1851", 201851, 60, 50, 961),
        (Pose::KingWalk, "dat_1855", 201855, 30, 23, 481),
        (Pose::KingDie, "dat_1848", 201848, 30, 33, 481),
        (Pose::KingDecay, "dat_1852", 201852, 30, 1000, 481),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            (source.id, source.stride, source.millis, source.frames()),
            (id, stride, millis, frames)
        );
        assert_eq!(source.manifest_source(), format!("de/{name}.sld"));
        for row in 0..16 {
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
    for (pose, expected) in [
        (Pose::ArcherAttack, 15),
        (Pose::SkirmisherAttack, 19),
        (Pose::CrossbowmanAttack, 15),
        (Pose::EliteSkirmisherAttack, 19),
    ] {
        let release = release_frame(pose).unwrap();
        assert_eq!(release, expected, "native DE frame delay");
        assert!(release > 0 && release < SPRITES[pose as usize].stride);
    }
    // Melee and siege poses release nothing mid-animation.
    assert_eq!(release_frame(Pose::KnightAttack), None);
    assert_eq!(release_frame(Pose::MangonelAttack), None);
}

#[test]
fn general_uses_distinct_full_paladin_animations() {
    for (pose, name, stride) in [
        (Pose::GeneralStand, "dat_3449", 45),
        (Pose::GeneralWalk, "dat_3453", 30),
        (Pose::GeneralAttack, "dat_3443", 45),
        (Pose::GeneralDie, "dat_3446", 45),
        (Pose::GeneralDecay, "dat_3450", 30),
    ] {
        let s = SPRITES[pose as usize];
        assert_eq!(s.manifest_source(), format!("de/{name}.sld"));
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
