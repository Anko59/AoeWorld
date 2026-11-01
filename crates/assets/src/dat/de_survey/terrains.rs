//! Current DE terrain directory, located as one validated chain of 200 records.
use super::{Reader, debug};
use crate::{Error, invalid};
#[derive(Clone, Debug)]
pub struct Terrain {
    pub id: u16,
    pub name: String,
    pub texture: String,
    pub overlay: String,
    pub dimensions: [u16; 2],
    pub draw: i16,
    pub units: Vec<i16>,
}
fn record(r: &mut Reader<'_>, id: u16) -> Result<Terrain, Error> {
    if r.u8()? > 1 || r.u8()? > 1 {
        return Err(r.fail("terrain flags"));
    }
    r.skip(6)?;
    let name = debug(r)?;
    let texture = debug(r)?;
    if texture
        .bytes()
        .any(|b| !(b.is_ascii_alphanumeric() || b == b'_'))
    {
        return Err(r.fail("terrain texture"));
    }
    r.skip(28)?;
    let overlay = debug(r)?;
    if !overlay.is_empty()
        && !overlay.strip_suffix(".png").is_some_and(|name| {
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err(r.fail("terrain overlay"));
    }
    r.skip(8 + 22 + 19 * 6)?;
    let draw = r.i16()?;
    let dimensions = [r.u16()?, r.u16()?];
    if !(-1..200).contains(&draw) || dimensions.iter().any(|&n| n > 256) {
        return Err(r.fail("terrain dimensions or alias"));
    }
    r.skip(30 * 2)?;
    let mut units = Vec::new();
    for _ in 0..30 {
        units.push(r.i16()?);
    }
    r.skip(30 * 3)?;
    let used = r.u16()? as usize;
    if used > 30 {
        return Err(r.fail("terrain units"));
    }
    units.truncate(used);
    r.skip(2)?;
    Ok(Terrain {
        id,
        name,
        texture,
        overlay,
        dimensions,
        draw,
        units,
    })
}
pub fn find(bytes: &[u8]) -> Result<Vec<Terrain>, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0") {
        return Err(invalid("DE terrain", 0, "unsupported version"));
    }
    for (at, head) in bytes.windows(2).enumerate().skip(8) {
        if head != [0x60, 0x0a] || at < 8 {
            continue;
        }
        let mut r = Reader::new(bytes, at - 8);
        if let Ok(first) = record(&mut r, 0) {
            if first.texture.is_empty() {
                continue;
            }
            let mut records = vec![first];
            for id in 1..200 {
                match record(&mut r, id) {
                    Ok(t) => records.push(t),
                    Err(_) => break,
                }
            }
            if records.len() == 200 {
                return Ok(records);
            }
        }
    }
    Err(invalid(
        "DE terrain",
        0,
        "missing validated terrain directory",
    ))
}
#[cfg(test)]
mod tests;
