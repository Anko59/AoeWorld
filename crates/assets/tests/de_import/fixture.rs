//! Synthetic current DE actors: numeric topology, never original pixels.
use std::{fs, io::Write, path::Path};
#[path = "units.rs"]
mod units;

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
    units::append(&mut records);
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&records).unwrap();
    fs::write(dat.join("empires2_x2_p1.dat"), encoder.finish().unwrap()).unwrap();
}
