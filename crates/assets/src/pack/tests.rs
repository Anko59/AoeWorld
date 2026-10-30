use super::atlas::{PageData, save_page};
use super::*;
use crate::slp::Pixel;

#[path = "tests/buildings.rs"]
mod buildings;

#[test]
fn oversized_pack_is_rejected_before_rendering_or_creating_output() {
    let temp = tempfile::tempdir().unwrap();
    for counts in [vec![MAX_FRAMES, 1], vec![usize::MAX, 1]] {
        let sprites: Vec<_> = counts
            .into_iter()
            .map(|frames| SpriteRef {
                id: "synthetic".into(),
                hash: "a".repeat(64),
                frames,
            })
            .collect();
        let output = temp.path().join("packs");
        let result = write_pack(&output, "b".repeat(64), &sprites, Vec::new(), |_, _| {
            panic!("oversized inventory must not decode frames")
        });
        assert!(result.is_err());
        assert!(!output.exists());
    }
}

#[test]
fn atlas_masks_and_verifier_reject_tampering() {
    let temp = tempfile::tempdir().unwrap();
    let frame = Frame {
        width: 3,
        height: 1,
        hotspot_x: -1,
        hotspot_y: 0,
        pixels: vec![Pixel::Color(0), Pixel::Player(7), Pixel::Outline1],
    };
    let mut page = PageData::new();
    assert_eq!(
        page.place(&frame, &Palette(vec![[12, 34, 56]])).unwrap(),
        Some((1, 1))
    );
    let pixel = (PAGE + 1) * 4;
    assert_eq!(&page.color[pixel..pixel + 4], &[12, 34, 56, 255]);
    assert_eq!(&page.player[pixel + 4..pixel + 8], &[7, 0, 0, 255]);
    assert_eq!(&page.outline[pixel + 8..pixel + 12], &[1, 0, 0, 255]);
    let pages = vec![save_page(temp.path(), 0, page).unwrap()];
    let manifest = Manifest {
        version: 1,
        converter: "test".to_owned(),
        input_hash: "a".repeat(64),
        pages,
        buildings: Vec::new(),
        building_data: "00".into(),
        frames: vec![FrameRecord {
            source: "fixture.slp".to_owned(),
            source_hash: "b".repeat(64),
            frame: 0,
            page: 0,
            x: 1,
            y: 1,
            width: 3,
            height: 1,
            anchor_x: -1,
            anchor_y: 0,
        }],
    };
    fs::write(
        temp.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert_eq!(verify(temp.path()).unwrap().frames.len(), 1);
    fs::write(temp.path().join("color-000.png"), b"tampered").unwrap();
    assert!(verify(temp.path()).is_err());
}

#[test]
fn invalid_output_path_is_rejected() {
    assert!(output_path(Path::new("../outside")).is_err());
    assert!(output_path(Path::new("local-assets/packs")).is_ok());
}

#[test]
fn player_blending_preserves_partial_alpha_and_opaque_trial_shades() {
    assert_eq!(
        blend_player([200, 0, 200, 0], 7, 119, [65, 145, 245]),
        [65, 145, 245, 119]
    );
    assert_eq!(
        blend_player([117, 120, 115, 128], 7, 119, [65, 145, 245]),
        [84, 136, 198, 187]
    );
    assert_eq!(
        blend_player([99, 99, 99, 99], 7, 1, [65, 145, 245]),
        [99, 99, 100, 100]
    );
    assert_eq!(
        blend_player([12, 34, 56, 78], 7, 0, [65, 145, 245]),
        [12, 34, 56, 78]
    );
    for (ramp, expected) in [
        [42, 94, 159, 255],
        [45, 101, 171, 255],
        [48, 108, 183, 255],
        [51, 115, 195, 255],
        [55, 123, 208, 255],
        [58, 130, 220, 255],
        [61, 137, 232, 255],
        [65, 145, 245, 255],
    ]
    .into_iter()
    .enumerate()
    {
        for base in [[0; 4], [117, 120, 115, 255]] {
            assert_eq!(
                blend_player(base, ramp as u8, 255, [65, 145, 245]),
                expected,
                "trial ramp {ramp}"
            );
        }
    }
}

#[test]
fn team_colour_replaces_the_ramp_without_losing_mask_coverage() {
    assert_eq!(
        blend_player([0, 0, 0, 0], 7, 255, [240, 50, 40]),
        [240, 50, 40, 255]
    );
    assert_eq!(
        blend_player([0, 0, 0, 0], 7, 128, [240, 50, 40]),
        [240, 50, 40, 128]
    );
    assert_eq!(
        blend_player([20, 40, 60, 255], 7, 0, [240, 50, 40]),
        [20, 40, 60, 255]
    );
}
