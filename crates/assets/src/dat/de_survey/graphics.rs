//! Focused VER 8.9 graphic records (genieutils graphic.py layout).
//! Exact debug filenames identify records; bounded fields validate the match.
use super::{Error, Reader, debug};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Animation {
    pub file: String,
    pub sprite: i32,
    pub frames: u16,
    pub angles: u16,
    pub seconds: f32,
    pub replay_delay: f32,
}

fn record(bytes: &[u8], at: usize) -> Result<Animation, Error> {
    let mut r = Reader::new(bytes, at);
    let file = debug(&mut r)?;
    debug(&mut r)?; // particle effect
    let sprite = r.i32()?;
    r.skip(14)?; // loaded, colour, layer, selection rectangle
    let deltas = usize::from(r.u16()?);
    r.bounded(deltas, 256, "graphic deltas")?;
    r.skip(6)?; // sound and Wwise sound
    let sounds = r.u8()?;
    let frames = r.u16()?;
    let angles = r.u16()?;
    r.skip(4)?; // speed multiplier
    let seconds = r.f32()?;
    let replay_delay = r.f32()?;
    r.skip(5)?; // sequence, id, mirroring, editor
    if sprite < 0
        || !(1..=255).contains(&frames)
        || !(1..=64).contains(&angles)
        || !seconds.is_finite()
        || seconds <= 0.0
        || seconds > 60.0
        || !replay_delay.is_finite()
        || replay_delay < 0.0
        || sounds > 1
    {
        return Err(r.fail("invalid DE graphic topology or timing"));
    }
    r.skip(deltas * 16)?;
    if sounds != 0 {
        r.skip(usize::from(angles) * 24)?;
    }
    Ok(Animation {
        file,
        sprite,
        frames,
        angles,
        seconds,
        replay_delay,
    })
}

pub fn find(bytes: &[u8], file: &str) -> Result<Animation, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0") || file.len() > 256 {
        return Err(crate::invalid(
            "DE graphic",
            0,
            "unsupported version or filename",
        ));
    }
    let mut signature = vec![0x60, 0x0a];
    signature.extend_from_slice(&(file.len() as u16).to_le_bytes());
    signature.extend_from_slice(file.as_bytes());
    let mut matches = bytes
        .windows(signature.len())
        .enumerate()
        .filter_map(|(at, b)| (b == signature).then_some(at));
    let at = matches
        .next()
        .ok_or_else(|| crate::invalid("DE graphic", 0, format!("missing {file}")))?;
    if matches.next().is_some() {
        return Err(crate::invalid(
            "DE graphic",
            at,
            "ambiguous graphic filename",
        ));
    }
    record(bytes, at)
}

/// Available selected hero graphics for the local DAT table. Synthetic numeric
/// survey fixtures need not include the independent graphic section.
pub(super) fn heroes(bytes: &[u8]) -> Result<Vec<Animation>, Error> {
    crate::de::SELECTION
        .iter()
        .filter(|s| s.building == "general")
        .filter_map(|s| {
            let name = format!("{}_x1", s.stem);
            bytes
                .windows(name.len())
                .any(|w| w == name.as_bytes())
                .then(|| find(bytes, &name))
        })
        .collect()
}

#[cfg(test)]
mod tests;
