//! Bounded reader for the importer's compiled recipe view. No string trees or
//! floating-point JSON visitors are needed by the browser's startup path.
use super::manifest::{BuildingComposition, BuildingPart, FrameRecord};

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
        .chunks_exact(2)
        .map(|b| Ok(nibble(b[0])? << 4 | nibble(b[1])?))
        .collect::<Result<Vec<_>, &'static str>>()?;
    let mut r = Reader { bytes, pos: 0 };
    let count = r.array::<1>()?[0];
    if count > 32 {
        return Err("too many building recipes");
    }
    let mut buildings = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let unit_id = r.i32()?;
        let footprint = [r.float()?, r.float()?];
        let slp = r.i32()?;
        let foundation_size = [r.u16()?, r.u16()?];
        buildings.push(BuildingComposition {
            unit_id,
            footprint,
            foundation_slp: (slp >= 0).then_some(slp),
            foundation_size,
            parts: r.parts(frames)?,
            construction: r.parts(frames)?,
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
