//! Compiled startup view of the readable DAT recipes. Exactly one current
//! encoding: little-endian fields, hex in JSON, source frame indices rather
//! than repeated strings. The verifier checks it against the readable recipe.
use super::FrameRecord;
use crate::{
    Error,
    dat::{BuildingComposition, BuildingPart},
    invalid,
};

pub fn encode(buildings: &[BuildingComposition], frames: &[FrameRecord]) -> Result<String, Error> {
    if buildings.len() > crate::catalog::GAMEPLAY_BUILDING_IDS.len() {
        return Err(invalid("recipes", 0, "too many buildings"));
    }
    let mut bytes = vec![buildings.len() as u8];
    for building in buildings {
        bytes.extend(building.unit_id.to_le_bytes());
        for v in building.footprint {
            bytes.extend(v.to_le_bytes());
        }
        bytes.extend(building.foundation_slp.unwrap_or(-1).to_le_bytes());
        for v in building.foundation_size {
            bytes.extend(v.to_le_bytes());
        }
        parts(&mut bytes, &building.parts, frames)?;
        parts(&mut bytes, &building.construction, frames)?;
        if building.damage.len() > 7 {
            return Err(invalid("recipes", 0, "too many damage states"));
        }
        let mut shared = Vec::new();
        for part in building.damage.iter().flat_map(|d| &d.parts) {
            let mut key = part.clone();
            key.graphic_id = 0;
            key.slp_id = 0;
            key.layer = 0;
            key.offset = [0; 2];
            if !shared.contains(&key) {
                shared.push(key);
            }
        }
        parts(&mut bytes, &shared, frames)?;
        bytes.push(building.damage.len() as u8);
        for damage in &building.damage {
            bytes.extend(damage.damage_percent.to_le_bytes());
            bytes.push(damage.apply_mode);
            if damage.parts.len() > 256 {
                return Err(invalid("recipes", 0, "too many damage parts"));
            }
            bytes.extend((damage.parts.len() as u16).to_le_bytes());
            for part in &damage.parts {
                let mut key = part.clone();
                key.graphic_id = 0;
                key.slp_id = 0;
                key.layer = 0;
                key.offset = [0; 2];
                let index = shared
                    .iter()
                    .position(|p| *p == key)
                    .ok_or_else(|| invalid("recipes", 0, "missing shared damage part"))?;
                bytes.extend((index as u16).to_le_bytes());
                for offset in part.offset {
                    bytes.extend(offset.to_le_bytes());
                }
            }
        }
    }
    if bytes.len() > 32768 {
        return Err(invalid("recipes", 0, "compiled recipes exceed 32 KiB"));
    }
    const HEX: &[u8] = b"0123456789abcdef";
    Ok(bytes
        .into_iter()
        .flat_map(|b| [HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]])
        .map(char::from)
        .collect())
}

fn parts(bytes: &mut Vec<u8>, parts: &[BuildingPart], frames: &[FrameRecord]) -> Result<(), Error> {
    if parts.len() > 256 {
        return Err(invalid("recipes", 0, "too many parts"));
    }
    bytes.extend((parts.len() as u16).to_le_bytes());
    for part in parts {
        let record = if part.available {
            frames
                .iter()
                .position(|f| f.source == part.source && f.frame == 0)
                .map(|i| i as u32)
                .ok_or_else(|| invalid("recipes", 0, "missing available source"))?
        } else {
            u32::MAX
        };
        bytes.extend(record.to_le_bytes());
        bytes.extend(part.frames.to_le_bytes());
        bytes.extend(part.angles.to_le_bytes());
        bytes.push(u8::from(part.primary));
        bytes.extend(part.frame_rate.to_le_bytes());
        for v in part.offset {
            bytes.extend(v.to_le_bytes());
        }
        for v in part.misplacement {
            bytes.extend(v.to_le_bytes());
        }
    }
    Ok(())
}
