use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn thumbnail_uses_native_owner_weights_and_preserves_unmasked_art_and_shadows() {
    let colour = [12, 30, 220, 255, 80, 90, 100, 255, 0, 0, 0, 0];
    let mask = [0, 0, 0, 0, 7, 0, 0, 128, 0, 0, 0, 0];
    let shadow = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 70];
    let bytes = compressed::encode_frame([3, 1], [&colour, &mask, &shadow]).unwrap();
    let pixels = strategic_frame(&bytes, 3, 1, 0x4191f5).unwrap();
    assert_eq!(&pixels[..4], &colour[..4]);
    assert_eq!(
        &pixels[4..8],
        &aoe_assets::pack::blend_player([80, 90, 100, 255], 7, 128, [65, 145, 245])
    );
    assert_eq!(&pixels[8..], &shadow[8..]);
}

#[wasm_bindgen_test]
fn every_far_unit_miniature_uses_its_idle_dat_source_and_cell() {
    use aoe_assets::catalog::units::SPRITES;
    use aoe_protocol::UnitKind;

    let mut frames = Vec::new();
    for kind in UnitKind::ALL {
        let source = SPRITES[crate::unit_visuals::idle_pose(kind).unwrap() as usize];
        if frames
            .iter()
            .any(|(name, _)| *name == source.manifest_source())
        {
            continue;
        }
        for row in 0..source.rows() {
            let cell = source.source_frame(row, 0) as u32;
            frames.push((source.manifest_source(), cell));
        }
    }
    let mut fixture = manifest::fixture();
    let mut records = Vec::new();
    for (source, frame) in frames {
        let mut row = fixture["frames"][0].clone();
        row["source"] = source.into();
        row["frame"] = frame.into();
        records.push(row);
    }
    fixture["frames"] = records.into();
    let pack: Manifest = serde_json::from_value(fixture).unwrap();
    let selected = miniature_records(&pack);
    assert_eq!(selected.len(), UnitKind::ALL.len());
    for (kind, record) in UnitKind::ALL.into_iter().zip(selected) {
        let source = SPRITES[crate::unit_visuals::idle_pose(kind).unwrap() as usize];
        let record = &pack.frames[record.unwrap()];
        assert_eq!(record.source, source.manifest_source(), "{kind:?}");
        assert_eq!(
            record.frame,
            source.source_frame(source.rows() / 2, 0) as u32
        );
    }
    for (a, b) in [
        (UnitKind::Villager, UnitKind::Militia),
        (UnitKind::FishingShip, UnitKind::WarGalley),
        (UnitKind::Crossbowman, UnitKind::Knight),
        (UnitKind::Galleon, UnitKind::CannonGalleon),
        (UnitKind::Scorpion, UnitKind::HandCannoneer),
        (UnitKind::BombardCannon, UnitKind::SiegeOnager),
    ] {
        let records = miniature_records(&pack);
        assert_ne!(records[a as usize], records[b as usize], "{a:?} and {b:?}");
    }
}

#[wasm_bindgen_test]
fn missing_far_unit_art_has_no_substitute() {
    let pack: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
    assert!(
        miniature_records(&pack)
            .into_iter()
            .all(|row| row.is_none())
    );
}
