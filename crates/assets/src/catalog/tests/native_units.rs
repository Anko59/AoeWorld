use crate::catalog::units::{Pose, SPRITES};

#[test]
fn military_units_keep_complete_native_de_direction_blocks() {
    // The army roster, including all current promotion tiers and the Vassal.
    let slots = (0..4)
        .chain([29, 30, 31, 37])
        .chain(50..72)
        .chain(75..120)
        .chain(126..135);
    for slot in slots {
        let source = SPRITES[slot];
        assert!(source.manifest_source().starts_with("de/"), "slot {slot}");
        assert_eq!(source.rows(), 16, "slot {slot}");
        assert_eq!(source.sample_step(), 1, "slot {slot}");
        let cells = usize::from(source.stride) * 16;
        assert_eq!(source.selected_frames(), cells, "slot {slot}");
        assert!((cells..=cells + 1).contains(&(source.frames() as usize)));
        for row in 0..16 {
            for phase in 0..usize::from(source.stride) {
                assert_eq!(
                    source.source_frame(row, phase),
                    row * usize::from(source.stride) + phase
                );
            }
        }
    }
}

#[test]
fn militia_timelines_use_fractional_native_dat_durations() {
    for (pose, name, millis) in [
        (Pose::MilitiaStand, "dat_1102", 66.66667014360428),
        (Pose::MilitiaWalk, "dat_1106", 27.900001034140587),
        (Pose::MilitiaAttack, "dat_1096", 50.00000074505806),
        (Pose::MilitiaDie, "dat_1099", 33.33333507180214),
        (Pose::MilitiaDecay, "dat_1103", 1000.0),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(source.manifest_source(), format!("de/{name}.sld"));
        assert_eq!((source.frames(), source.stride), (481, 30));
        assert!((source.frame_millis() - millis).abs() < 0.000001);
    }
}

#[test]
fn every_spawned_unit_pose_uses_all_native_cells_and_dat_timing() {
    assert_eq!(SPRITES.len(), 167);
    for (pose, source) in SPRITES.iter().copied().enumerate() {
        if matches!(pose, 72 | 73 | 120) {
            continue;
        }
        assert!(source.manifest_source().starts_with("de/"), "pose {pose}");
        assert_eq!(source.sample_step(), 1, "pose {pose}");
        assert!(matches!(source.rows(), 1 | 8 | 16 | 32), "pose {pose}");
        assert_eq!(
            source.selected_frames(),
            usize::from(source.stride) * source.rows()
        );
        assert!(source.frame_millis().is_finite() && source.frame_millis() >= 0.0);
        for row in 0..source.rows() {
            for phase in 0..usize::from(source.stride) {
                let native_stride = source.frames() as usize / source.rows();
                let expected = if source.stride == 1 && native_stride > 1 {
                    row * native_stride + native_stride - 1
                } else {
                    row * usize::from(source.stride) + phase
                };
                assert_eq!(source.source_frame(row, phase), expected);
            }
        }
    }
}
