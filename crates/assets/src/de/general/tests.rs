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
fn general_import_keeps_exact_native_facing_cells_and_all_planes() {
    for (role, count, cells) in [
        ("idle", 721, [180, 270, 360, 450, 540]),
        ("walk", 480, [120, 180, 240, 300, 360]),
        ("death", 721, [224, 314, 404, 494, 584]),
    ] {
        let native: Vec<_> = (0..count).map(frame).collect();
        let expected: Vec<_> = cells.map(|n| native[n].clone()).into();
        assert_eq!(super::sample(role, native).unwrap(), expected);
    }
}

#[test]
fn general_import_refuses_partial_or_changed_native_topology() {
    for count in [0, 1, 720, 722] {
        assert!(super::sample("idle", (0..count).map(frame).collect()).is_err());
    }
    for count in [479, 481] {
        assert!(super::sample("walk", (0..count).map(frame).collect()).is_err());
    }
    assert!(super::sample("unknown", (0..721).map(frame).collect()).is_err());
}

#[test]
fn general_catalog_uses_five_current_logical_facing_cells() {
    for pose in [
        Pose::GeneralStand,
        Pose::GeneralWalk,
        Pose::GeneralAttack,
        Pose::GeneralDie,
        Pose::GeneralDecay,
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(source.frames(), 5);
        for row in 0..5 {
            assert_eq!(source.source_frame(row, 0), row);
        }
    }
}
