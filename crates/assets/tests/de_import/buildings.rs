//! One synthetic DAT building family drives every gameplay recipe.
use super::*;
pub(super) fn append(records: &mut Vec<u8>) {
    let graphic = 28000_i16;
    let fire = 28001_i16;
    graphic_record(records, graphic, "b_dark_house_age1_x1", "");
    graphic_record(records, fire, "None", "fire_fixture");
    for id in aoe_assets::catalog::GAMEPLAY_BUILDING_IDS {
        let id = id as i16;
        let mut b = units::record(id, &[graphic, -1, -1, -1, -1, -1, -1, -1], -1);
        if id == 142 {
            b[157] = 1;
            b.splice(158..158, fire.to_le_bytes().into_iter().chain([75, 0, 0]));
        }
        finish(&mut b, -1);
        records.extend(b);
    }
}

fn graphic_record(records: &mut Vec<u8>, graphic: i16, file: &str, particle: &str) {
    for name in [file, particle] {
        records.extend([0x60, 0x0a]);
        records.extend((name.len() as u16).to_le_bytes());
        records.extend(name.as_bytes());
    }
    records.extend((-1_i32).to_le_bytes());
    records.extend([0; 23]);
    records.extend(1_u16.to_le_bytes());
    records.extend(1_u16.to_le_bytes());
    records.extend([0; 13]);
    records.extend(graphic.to_le_bytes());
    records.extend([0; 2]);
}

pub(super) fn finish(b: &mut Vec<u8>, construction: i16) {
    b[0] = 80;
    b[29..33].copy_from_slice(&1_f32.to_le_bytes());
    b[33..37].copy_from_slice(&1_f32.to_le_bytes());
    b.extend([0; 32 + 18 + 2 + 97]);
    b.extend(construction.to_le_bytes());
    for _ in 0..5 {
        b.extend((-1_i16).to_le_bytes());
    }
    b.extend([0; 4]);
    for _ in 0..4 {
        b.extend((-1_i16).to_le_bytes());
    }
    b.push(0);
    for _ in 0..4 {
        b.extend((-1_i16).to_le_bytes());
        b.extend([0; 8]);
    }
    b.extend([0; 33]);
}
