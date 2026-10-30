//! Bounded reader for the importer's compiled recipe view. No string trees or
//! floating-point JSON visitors are needed by the browser's startup path.
use super::manifest::{BuildingComposition, BuildingDamage, BuildingPart, FrameRecord};

struct Reader {
    bytes: Vec<u8>,
    pos: usize,
}
impl Reader {
    fn array<const N: usize>(&mut self) -> Result<[u8; N], &'static str> {
        let mut value = [0; N];
        value.copy_from_slice(
            self.bytes
                .get(self.pos..self.pos + N)
                .ok_or("truncated building recipe")?,
        );
        self.pos += N;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16, &'static str> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn i16(&mut self) -> Result<i16, &'static str> {
        Ok(i16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn i32(&mut self) -> Result<i32, &'static str> {
        Ok(i32::from_le_bytes(self.array()?))
    }
    fn float(&mut self) -> Result<f32, &'static str> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn parts(&mut self, frames: &[FrameRecord]) -> Result<Vec<BuildingPart>, &'static str> {
        let count = self.u16()?;
        if count > 256 {
            return Err("too many building parts");
        }
        let mut parts = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let record = match self.u32()? {
                u32::MAX => None,
                i if frames.get(i as usize).is_some_and(|f| f.frame == 0) => Some(i as usize),
                _ => return Err("invalid building frame index"),
            };
            parts.push(BuildingPart {
                record,
                frames: self.u16()?,
                angles: self.u16()?,
                primary: match self.array::<1>()?[0] {
                    0 => false,
                    1 => true,
                    _ => return Err("invalid primary graphic flag"),
                },
                frame_rate: self.float()?,
                replay_delay: self.float()?,
                offset: [self.i32()?, self.i32()?],
                misplacement: [self.float()?, self.float()?],
            });
        }
        Ok(parts)
    }
}

fn nibble(byte: u8) -> Result<u8, &'static str> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("invalid building recipe hex"),
    }
}

pub(super) fn decode(
    hex: &str,
    frames: &[FrameRecord],
) -> Result<Vec<BuildingComposition>, &'static str> {
    if hex.len() > 64 * 1024 || hex.len() % 2 != 0 {
        return Err("invalid building recipe size");
    }
    let bytes = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| Ok(nibble(b[0])? << 4 | nibble(b[1])?))
        .collect::<Result<Vec<_>, &'static str>>()?;
    let mut r = Reader { bytes, pos: 0 };
    let count = r.array::<1>()?[0];
    if usize::from(count) > aoe_assets::catalog::GAMEPLAY_BUILDING_IDS.len() {
        return Err("too many building recipes");
    }
    let mut buildings = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let unit_id = r.i32()?;
        let footprint = [r.float()?, r.float()?];
        let slp = r.i32()?;
        let foundation_size = [r.u16()?, r.u16()?];
        let parts = r.parts(frames)?;
        let construction = r.parts(frames)?;
        let state_count = r.array::<1>()?[0];
        if state_count > 8 {
            return Err("too many building states");
        }
        let mut states = Vec::new();
        for _ in 0..state_count {
            let role = r.array::<1>()?[0];
            if role > 7 || states.iter().any(|(old, _)| *old == role) {
                return Err("invalid building state");
            }
            states.push((role, r.parts(frames)?));
        }
        let shared = r.parts(frames)?;
        if shared.iter().any(|part| part.offset != [0; 2]) {
            return Err("invalid shared damage offset");
        }
        let count = r.array::<1>()?[0];
        if count > 7 {
            return Err("too many damage states");
        }
        let mut damage = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let damage_percent = r.i16()?;
            let apply_mode = r.array::<1>()?[0];
            if !(0..=100).contains(&damage_percent) || apply_mode > 2 {
                return Err("invalid damage state");
            }
            let count = r.u16()?;
            if count > 256 {
                return Err("too many damage parts");
            }
            let mut parts = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                let index = usize::from(r.u16()?);
                let mut part = shared
                    .get(index)
                    .ok_or("invalid damage part index")?
                    .clone();
                part.offset = [r.i32()?, r.i32()?];
                parts.push(part);
            }
            damage.push(BuildingDamage {
                damage_percent,
                apply_mode,
                parts,
            });
        }
        buildings.push(BuildingComposition {
            unit_id,
            footprint,
            foundation_slp: (slp >= 0).then_some(slp),
            foundation_size,
            parts,
            construction,
            states,
            damage,
        });
    }
    if r.pos != r.bytes.len() {
        return Err("trailing building recipe bytes");
    }
    Ok(buildings)
}

#[cfg(test)]
#[path = "tests/recipes.rs"]
mod tests;
