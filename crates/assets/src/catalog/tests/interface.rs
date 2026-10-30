use super::*;
use crate::pack::{AtlasPage, FrameRecord};

#[test]
fn interface_catalog_uses_exact_frames_and_rejects_incomplete_sheets() {
    let hash = "a".repeat(64);
    let mut manifest = Manifest {
        terrains: Vec::new(),
        chunks: Vec::new(),
        version: 1,
        converter: "synthetic".into(),
        input_hash: hash.clone(),
        pages: vec![AtlasPage {
            color: "color-000.png".into(),
            color_hash: hash.clone(),
            player: "p.png".into(),
            player_hash: hash.clone(),
            shadow: "s.png".into(),
            shadow_hash: hash.clone(),
            outline: "o.png".into(),
            outline_hash: hash.clone(),
            width: 2048,
            height: 2048,
        }],
        frames: vec![],
        buildings: vec![],
        building_data: String::new(),
    };
    assert!(art(&manifest, 50730, 11).unwrap().is_none());
    manifest.frames.push(FrameRecord {
        native: None,
        source: "interfac.drs:[32, 112, 108, 115]:50730".into(),
        source_hash: hash,
        frame: 11,
        page: 0,
        x: 70,
        y: 90,
        width: 36,
        height: 36,
        anchor_x: 0,
        anchor_y: 0,
    });
    let selected = art(&manifest, 50730, 11).unwrap().unwrap();
    assert_eq!((selected.x, selected.y, selected.width), (70, 90, 36));
    assert_eq!(selected.page, "color-000.png");
    assert!(art(&manifest, 50730, 12).is_err());
    assert!(crop(selected.clone(), 35, 0, 2, 2).is_err());
    let cropped = crop(selected, 1, 2, 24, 19).unwrap();
    assert_eq!(
        (cropped.x, cropped.y, cropped.width, cropped.height),
        (71, 92, 24, 19)
    );
}

#[test]
fn interface_commands_use_command_frames_and_the_garrison_cursor() {
    for (name, slp, frame) in [
        ("stop", 50721, 3),
        ("build", 50721, 30),
        ("military", 50721, 31),
        ("repair", 50721, 28),
        ("garrison", 51000, 13),
        ("ungarrison", 50721, 2),
        ("move", 51000, 0),
        ("attack", 51000, 4),
        ("build-cursor", 51000, 7),
    ] {
        let source = command_icon(name).unwrap();
        assert_eq!((source.slp, source.frame), (slp, frame), "{name}");
    }
    assert!(command_icon("unknown").is_none());
}

#[test]
fn interface_castle_building_families_are_in_the_gameplay_catalog() {
    for id in [82, 104, 209, 49] {
        assert!(
            crate::catalog::GAMEPLAY_BUILDING_IDS.contains(&id),
            "missing Castle DAT building {id}"
        );
    }
}
