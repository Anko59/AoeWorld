use crate::{
    catalog::units::{Pose, SPRITES},
    sld::{Frame, Layer},
};

fn frame(index: usize) -> Frame {
    let layer = Layer {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        pixels: vec![[index as u8, 2, 3, 255]],
    };
    Frame {
        width: 32,
        height: 32,
        anchor_x: index as u16,
        anchor_y: 17,
        index: index as u16,
        main: Some(layer.clone()),
        shadow: Some(layer.clone()),
        player: Some(layer),
    }
}

#[test]
fn general_import_keeps_every_native_frame_and_plane() {
    for (role, count) in [
        ("idle", 721),
        ("walk", 481),
        ("attack", 721),
        ("death", 721),
        ("decay", 481),
    ] {
        let native: Vec<_> = (0..count).map(frame).collect();
        assert_eq!(super::sample(role, native.clone()).unwrap(), native);
    }
}

#[test]
fn general_import_refuses_partial_or_changed_native_topology() {
    for role in ["idle", "walk", "attack", "death", "decay"] {
        let count = if matches!(role, "walk" | "decay") {
            481
        } else {
            721
        };
        for bad in [0, 1, count - 1, count + 1] {
            assert!(super::sample(role, (0..bad).map(frame).collect()).is_err());
        }
    }
    assert!(super::sample("unknown", (0..721).map(frame).collect()).is_err());
}

#[test]
fn general_catalog_retains_full_native_animation_in_every_game_facing() {
    for (pose, role, phases) in [
        (Pose::GeneralStand, "idle", 45),
        (Pose::GeneralWalk, "walk", 30),
        (Pose::GeneralAttack, "attack", 45),
        (Pose::GeneralDie, "death", 45),
        (Pose::GeneralDecay, "decay", 30),
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            source.manifest_source(),
            format!("de/u_cav_hero_frankish_paladin_{role}A_x1.sld")
        );
        assert_eq!(usize::from(source.stride), phases);
        assert_eq!(source.sample_step(), 1);
        for row in 0..16 {
            for phase in 0..phases {
                assert_eq!(source.source_frame(row, phase), row * phases + phase);
            }
        }
    }
}

#[test]
fn real_general_matches_dat_and_has_distinct_native_frames_in_every_pose() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let root = std::path::Path::new(&root);
    crate::de::native::validate_dat(root).unwrap();
    for item in crate::de::SELECTION
        .iter()
        .filter(|s| s.building == "general")
    {
        let path = root
            .join("resources/_common/drs/graphics")
            .join(format!("{}_x1.sld", item.stem));
        let frames = crate::sld::parse(&std::fs::read(path).unwrap()).unwrap();
        let period = if matches!(item.role, "walk" | "decay") {
            30
        } else {
            45
        };
        assert_eq!(frames.len(), period * 16 + 1);
        for row in 0..16 {
            assert!(
                frames[row * period..(row + 1) * period]
                    .windows(2)
                    .any(|w| {
                        w[0].anchor_x != w[1].anchor_x
                            || w[0].anchor_y != w[1].anchor_y
                            || w[0].main != w[1].main
                            || w[0].shadow != w[1].shadow
                            || w[0].player != w[1].player
                    }),
                "{} row {row}",
                item.role
            );
        }
        assert!(frames.iter().any(|f| f.player.is_some()));
        assert_eq!(
            super::sample(item.role, frames).unwrap().len(),
            period * 16 + 1
        );
    }
}
