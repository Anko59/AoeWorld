//! Synthetic current DE actors: numeric topology, never original pixels.
use std::{fs, io::Write, path::Path};
#[path = "buildings.rs"]
mod buildings;
#[path = "units.rs"]
mod units;

pub(super) const HERO_PORTRAITS: [i16; 4] = [432, 200, 198, 170];

pub(super) fn actors(install: &Path) {
    let graphics = install.join("resources/_common/drs/graphics");
    let dat = install.join("resources/_common/dat");
    fs::create_dir_all(&graphics).unwrap();
    fs::create_dir_all(&dat).unwrap();
    let mut records = b"VER 8.9\0".to_vec();
    let mut sources = std::collections::BTreeMap::new();
    for source in aoe_assets::catalog::units::SPRITES {
        if source.id < 200_000 {
            continue;
        }
        sources
            .entry(source.id)
            .and_modify(|s: &mut aoe_assets::catalog::units::ActorSource| {
                if source.stride > s.stride {
                    *s = source;
                }
            })
            .or_insert(source);
    }
    for (id, source) in sources {
        let graphic = (id - 200_000) as i16;
        let name = format!("synthetic_{graphic}_x1");
        for text in [name.as_str(), ""] {
            records.extend_from_slice(&[0x60, 0x0a]);
            records.extend_from_slice(&(text.len() as u16).to_le_bytes());
            records.extend_from_slice(text.as_bytes());
        }
        records.extend_from_slice(&i32::from(graphic).to_le_bytes());
        records.extend_from_slice(&[0; 23]);
        records.extend_from_slice(&u16::from(source.stride).to_le_bytes());
        records.extend_from_slice(&(source.rows() as u16).to_le_bytes());
        records.extend_from_slice(&0_f32.to_le_bytes());
        records.extend_from_slice(&((source.frame_millis() / 1000.0) as f32).to_le_bytes());
        records.extend_from_slice(&source.replay_delay_seconds().to_le_bytes());
        records.push(0);
        records.extend_from_slice(&graphic.to_le_bytes());
        records.extend_from_slice(&[0; 2]);
        let cell = super::sld();
        let count = source.frames() as u16;
        let mut sld = cell[..16].to_vec();
        sld[6..8].copy_from_slice(&count.to_le_bytes());
        for _ in 0..count {
            sld.extend_from_slice(&cell[16..]);
        }
        fs::write(graphics.join(format!("{name}.sld")), sld).unwrap();
    }
    let cell = super::sld();
    let mut stone = cell[..16].to_vec();
    stone[6..8].copy_from_slice(&30_u16.to_le_bytes());
    for _ in 0..30 {
        stone.extend_from_slice(&cell[16..]);
    }
    fs::write(graphics.join("p_mangonel_x1.sld"), stone).unwrap();
    let mut bolt = cell[..16].to_vec();
    bolt[6..8].copy_from_slice(&32_u16.to_le_bytes());
    for _ in 0..32 {
        bolt.extend_from_slice(&cell[16..]);
    }
    fs::write(graphics.join("p_bolt_x1.sld"), bolt).unwrap();
    units::append(&mut records);
    buildings::append(&mut records);
    for id in aoe_assets::catalog::ENVIRONMENT_UNIT_IDS {
        let mut r = units::record(id as i16, &[28000, -1, -1, -1, -1, -1, -1, -1], -1);
        r[0] = 10;
        r[29..33].copy_from_slice(&1_f32.to_le_bytes());
        r[33..37].copy_from_slice(&1_f32.to_le_bytes());
        records.extend(r);
    }
    // Resource portraits are read from the DAT icon index, so the importer needs these units.
    for id in [59_i16, 349, 66, 102, 53] {
        let mut r = units::record(id, &[-1; 8], -1);
        r[0] = 10;
        r[29..33].copy_from_slice(&1_f32.to_le_bytes());
        r[33..37].copy_from_slice(&1_f32.to_le_bytes());
        records.extend(r);
    }
    for id in 0..200 {
        records.extend([1, 0, 0, 0, 0, 0, 0, 0]);
        for text in ["Ground", "g_fm1"] {
            records.extend(0xa60_u16.to_le_bytes());
            records.extend((text.len() as u16).to_le_bytes());
            records.extend(text.as_bytes());
        }
        records.extend([0; 28]);
        records.extend([0x60, 0xa, 0, 0]);
        records.extend([0; 8 + 22 + 19 * 6]);
        records.extend((-1_i16).to_le_bytes());
        records.extend(if id < 134 { [10, 0, 10, 0] } else { [0; 4] });
        records.extend([0; 30 * 7 + 4]);
    }
    fs::write(graphics.join("b_dark_house_age1_x1.sld"), super::sld()).unwrap();
    // Current DAT research identities and synthetic DDS cells complete the icon inventory.
    for language in [
        7102_i32, 7204, 7176, 7100, 7098, 7261, 7246, 7282, 7068, 7076, 7082, 7150, 7209, 7013,
        7190, 7180, 7050, 7093, 7321, 7189, 7014, 7055, 7276, 7211, 7101, 7008, 7230, 7067, 7074,
        7081, 7172, 7208, 7022, 7034, 7103, 7217, 7260, 7414, 7233, 7207, 7259, 7417, 7096, 7263,
        7262, 7075, 7077, 7080, 7151, 7216, 7012, 7231, 7277,
    ] {
        records.extend([0; 33]);
        records.extend(language.to_le_bytes());
        records.extend((language + 1000).to_le_bytes());
        records.extend([0; 6]); // effect, type, and icon
        records.extend([0; 8]);
        records.extend([0x60, 0x0a, 4, 0]);
        records.extend(b"TEST");
        records.push(0);
        records.extend(1_u16.to_le_bytes());
        records.extend([0; 9]);
    }
    for (directory, file) in [("units", "0_50730.dds"), ("tech", "0_test.dds")] {
        let path = install.join("widgetui/textures/ingame").join(directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(file), super::dds()).unwrap();
    }
    native_interface(install);
    // Squad cards resolve native hero portraits independently of actor timelines.
    for id in HERO_PORTRAITS {
        fs::write(
            install.join(format!("widgetui/textures/ingame/units/{id}_50730.dds")),
            super::dds(),
        )
        .unwrap();
    }
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&records).unwrap();
    fs::write(dat.join("empires2_x2_p1.dat"), encoder.finish().unwrap()).unwrap();
}

/// Minimal native interface sources: every icon, command, resource, panel and menu file the
/// importer requires, so a missing source still rejects the install.
fn native_interface(install: &Path) {
    let ingame = install.join("widgetui/textures/ingame");
    let png = {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255; 16])
            .unwrap();
        bytes
    };
    let write = |path: std::path::PathBuf, bytes: &[u8]| {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    };
    write(ingame.join("buildings/0_test.dds"), &super::dds());
    for index in [0, 1, 2, 3, 28, 30, 31, 32, 41, 4, 35, 43, 62, 63, 84, 74] {
        write(ingame.join(format!("actions/{index:03}_.png")), &png);
    }
    for name in [
        "resource_food",
        "resource_wood",
        "resource_stone",
        "resource_gold",
        "pop",
    ] {
        write(ingame.join(format!("icons/{name}.png")), &png);
    }
    for name in [
        "command-panel_extended",
        "single-selection-panel",
        "map-panel",
        "topbar",
    ] {
        write(ingame.join(format!("panels/WEST/{name}.png")), &png);
    }
    for name in ["blank_icon", "frame_regular"] {
        write(ingame.join(format!("panels/{name}.png")), &png);
    }
    write(
        install.join("widgetui/textures/backgrounds/mainmenu_bg.dds"),
        &super::dds(),
    );
}
