use super::*;

#[test]
fn interface_unit_icon_id_is_signed_and_independent_of_graphic_id() {
    let mut bytes = fixture(0);
    let name = bytes.windows(5).position(|w| w == b"HOUS\0").unwrap();
    let start = name - (146 + 2 * 5 + 6);
    bytes[start + 47..start + 49].copy_from_slice(&17i16.to_le_bytes());
    assert_eq!(parse(&bytes).unwrap().unit(0).unwrap().icon_id, 17);
    bytes[start + 47..start + 49].copy_from_slice(&(-1i16).to_le_bytes());
    assert_eq!(parse(&bytes).unwrap().unit(0).unwrap().icon_id, -1);
}

#[test]
fn interface_technology_table_preserves_slot_and_icon_and_rejects_truncation() {
    let mut bytes = Vec::new();
    le16(&mut bytes, 2);
    for (icon, name) in [(22i16, "SYNTH A"), (-1, "SYNTH B")] {
        bytes.extend_from_slice(&[0; 12 + 15 + 2 + 4]);
        le16(&mut bytes, 109); // research location
        le16(&mut bytes, 7000); // language name
        le16(&mut bytes, 8000);
        le16(&mut bytes, 25); // time
        le16(&mut bytes, 7); // effect
        le16(&mut bytes, 0); // type
        le16(&mut bytes, icon);
        bytes.push(4); // grid button
        bytes.extend_from_slice(&[0; 12]);
        le16(&mut bytes, name.len() as i16);
        bytes.extend_from_slice(name.as_bytes());
    }
    let techs = technologies::decode(&bytes, 0).unwrap();
    assert_eq!(
        (techs[0].id, techs[0].icon_id, techs[0].location),
        (0, 22, 109)
    );
    assert_eq!((techs[1].id, techs[1].icon_id), (1, -1));
    assert_eq!(techs[1].name, "SYNTH B");
    assert!(technologies::decode(&bytes[..bytes.len() - 1], 0).is_err());
    assert!(technologies::decode(&[255, 255], 0).is_err());
}

#[test]
fn interface_catalog_resolves_dat_ids_to_the_correct_sheets() {
    let mut bytes = fixture(0);
    let name = bytes.windows(5).position(|w| w == b"HOUS\0").unwrap();
    let start = name - (146 + 2 * 5 + 6);
    bytes[start + 47..start + 49].copy_from_slice(&34i16.to_le_bytes());
    let file = parse(&bytes).unwrap();
    let art = crate::catalog::interface::unit_icon(&file, 0).unwrap();
    assert_eq!((art.slp, art.frame), (50706, 34));
    let art = crate::catalog::interface::unit_icon(&file, 2).unwrap();
    assert_eq!((art.slp, art.frame), (50730, 0));
    assert!(crate::catalog::interface::unit_icon(&file, 999).is_none());
}

#[test]
fn interface_catalog_exports_naval_units_and_castle_research() {
    use crate::pack::{AtlasPage, FrameRecord, Manifest};
    let mut dat = parse(&fixture(0)).unwrap();
    let template = dat.units[0].clone();
    let ships = [539, 529, 527, 545, 21];
    dat.units = ships
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let mut unit = template.clone();
            unit.id = *id;
            unit.kind = 70;
            unit.icon_id = index as i16;
            unit
        })
        .collect();
    dat.unit_index = ships
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    dat.technologies = [102, 34]
        .iter()
        .enumerate()
        .map(|(index, id)| Technology {
            id: *id,
            name: "synthetic research".into(),
            icon_id: index as i16,
            location: 45,
        })
        .collect();
    let hash = "a".repeat(64);
    let manifest = Manifest {
        chunks: Vec::new(),
        version: 1,
        converter: "synthetic".into(),
        input_hash: hash.clone(),
        pages: vec![AtlasPage {
            color: "icons.png".into(),
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
        frames: [(50730, 5), (50729, 2)]
            .into_iter()
            .flat_map(|(sheet, count)| {
                let hash = hash.clone();
                (0..count).map(move |frame| FrameRecord {
                    native: None,
                    source: format!("interfac.drs:[32, 112, 108, 115]:{sheet}"),
                    source_hash: hash.clone(),
                    frame,
                    page: 0,
                    x: (frame * 36) as u16,
                    y: 0,
                    width: 36,
                    height: 36,
                    anchor_x: 0,
                    anchor_y: 0,
                })
            })
            .collect(),
        buildings: vec![],
        building_data: String::new(),
    };
    let catalog = crate::catalog::interface::build(&dat, &manifest).unwrap();
    for id in ships {
        assert_eq!(
            catalog.units.get(&id).map(|art| art.page.as_str()),
            Some("icons.png"),
            "naval icon {id}"
        );
    }
    for id in [102, 34] {
        assert_eq!(
            catalog.technologies.get(&id).map(|art| art.page.as_str()),
            Some("icons.png"),
            "research icon {id}"
        );
    }
}

#[test]
#[ignore = "requires the owner's local AoK trial; no original bytes in fixtures"]
fn interface_trial_dat_icon_ids_match_reviewed_sheets() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../local-assets/trial/Data/empires2.dat");
    let dat = decode(&std::fs::read(path).unwrap()).unwrap();
    assert!(dat.report.stopped.is_none());
    assert_eq!(dat.technologies.len(), 426);
    for (id, sheet, frame) in [
        (83, 50730, 15),
        (74, 50730, 8),
        (75, 50730, 10),
        (448, 50730, 64),
        (13, 50730, 24),
        (109, 50706, 28),
        (70, 50706, 34),
        (12, 50706, 2),
        (68, 50706, 19),
        (562, 50706, 40),
        (584, 50706, 39),
    ] {
        let icon = crate::catalog::interface::unit_icon(&dat, id).unwrap();
        assert_eq!((icon.slp, icon.frame), (sheet, frame), "unit {id}");
    }
    for (id, frame) in [
        (22, 6),
        (8, 69),
        (101, 30),
        (202, 70),
        (14, 2),
        (55, 15),
        (278, 87),
        (213, 79),
        (222, 85),
        (67, 17),
        (74, 63),
        (81, 66),
        (199, 34),
        (211, 49),
    ] {
        let tech = dat.technologies.iter().find(|t| t.id == id).unwrap();
        assert_eq!(tech.icon_id, frame, "technology {id}");
    }
}
