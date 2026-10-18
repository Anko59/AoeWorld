use super::atlas::{PageData, save_page};
use super::*;
use crate::slp::Pixel;

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
