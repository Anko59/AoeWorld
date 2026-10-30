//! Synthetic current DE hero install: numeric topology, never original pixels.
use std::{fs, io::Write, path::Path};

pub(super) fn hero(install: &Path) {
    let graphics = install.join("resources/_common/drs/graphics");
    let dat = install.join("resources/_common/dat");
    fs::create_dir_all(&graphics).unwrap();
    fs::create_dir_all(&dat).unwrap();
    let mut records = b"VER 8.9\0".to_vec();
    for (role, sprite, frames, seconds) in [
        ("idle", 3776_i32, 45_u16, 3.5_f32 / 45.0),
        ("walk", 3780, 30, 1.1 / 30.0),
        ("attack", 3770, 45, 1.6 / 45.0),
        ("death", 3773, 45, 1.85 / 45.0),
        ("decay", 3777, 30, 1.0),
    ] {
        let name = format!("u_cav_hero_frankish_paladin_{role}A_x1");
        for text in [&name, ""] {
            records.extend_from_slice(&[0x60, 0x0a]);
            records.extend_from_slice(&(text.len() as u16).to_le_bytes());
            records.extend_from_slice(text.as_bytes());
        }
        records.extend_from_slice(&sprite.to_le_bytes());
        records.extend_from_slice(&[0; 23]);
        records.extend_from_slice(&frames.to_le_bytes());
        records.extend_from_slice(&16_u16.to_le_bytes());
        records.extend_from_slice(&0_f32.to_le_bytes());
        records.extend_from_slice(&seconds.to_le_bytes());
        records.extend_from_slice(&0_f32.to_le_bytes());
        records.extend_from_slice(&[0; 5]);
        let cell = super::sld();
        let count = frames * 16 + 1;
        let mut sld = cell[..16].to_vec();
        sld[6..8].copy_from_slice(&count.to_le_bytes());
        for _ in 0..count {
            sld.extend_from_slice(&cell[16..]);
        }
        fs::write(graphics.join(format!("{name}.sld")), sld).unwrap();
    }
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&records).unwrap();
    fs::write(dat.join("empires2_x2_p1.dat"), encoder.finish().unwrap()).unwrap();
}
