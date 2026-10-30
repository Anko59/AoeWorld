//! Focused VER 8.9 graphic records (genieutils graphic.py layout).
//! Exact debug filenames identify records; bounded fields validate the match.
use super::{Error, Reader, debug};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
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
        || seconds < 0.0
        || (seconds == 0.0 && frames != 1)
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
    find_record(bytes, file, None)
}

mod indexed;
pub use indexed::{Graphic, Part, indexed};

/// Several native graphics share a debug filename but have distinct sprite ids.
pub fn find_sprite(bytes: &[u8], file: &str, sprite: i32) -> Result<Animation, Error> {
    find_record(bytes, file, Some(sprite))
}

/// Scan once for a bounded set of native timelines, retaining request order.
/// Identical DAT aliases agree; a conflicting descriptor is an error.
pub fn find_sprites(bytes: &[u8], sources: &[(&str, i32)]) -> Result<Vec<Animation>, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0")
        || sources.len() > 256
        || sources.iter().any(|(file, _)| file.len() > 256)
    {
        return Err(crate::invalid(
            "DE graphic",
            0,
            "unsupported graphic selection",
        ));
    }
    let mut wanted = std::collections::BTreeMap::<&str, Vec<(usize, i32)>>::new();
    for (index, &(file, sprite)) in sources.iter().enumerate() {
        wanted.entry(file).or_default().push((index, sprite));
    }
    let mut found: Vec<Option<Animation>> = vec![None; sources.len()];
    for (at, head) in bytes.windows(4).enumerate() {
        if head[..2] != [0x60, 0x0a] {
            continue;
        }
        let length = usize::from(u16::from_le_bytes([head[2], head[3]]));
        if length > 256 {
            continue;
        }
        let start = at + 4;
        let Some(end) = start.checked_add(length) else {
            continue;
        };
        let Some(encoded) = bytes.get(start..end) else {
            continue;
        };
        let Ok(file) = std::str::from_utf8(encoded) else {
            continue;
        };
        let Some(requests) = wanted.get(file) else {
            continue;
        };
        let graphic = record(bytes, at)?;
        for &(index, sprite) in requests {
            if sprite != graphic.sprite {
                continue;
            }
            if found[index]
                .as_ref()
                .is_some_and(|previous| previous != &graphic)
            {
                return Err(crate::invalid(
                    "DE graphic",
                    at,
                    "ambiguous graphic identity",
                ));
            }
            found[index] = Some(graphic.clone());
        }
    }
    found
        .into_iter()
        .enumerate()
        .map(|(index, graphic)| {
            graphic.ok_or_else(|| {
                crate::invalid("DE graphic", 0, format!("missing {}", sources[index].0))
            })
        })
        .collect()
}

fn find_record(bytes: &[u8], file: &str, sprite: Option<i32>) -> Result<Animation, Error> {
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
    let matches = bytes
        .windows(signature.len())
        .enumerate()
        .filter_map(|(at, b)| (b == signature).then_some(at));
    let mut found = None;
    for at in matches {
        let graphic = record(bytes, at)?;
        if sprite.is_some_and(|id| graphic.sprite != id) {
            continue;
        }
        if let Some(previous) = &found {
            if sprite.is_none() || previous != &graphic {
                return Err(crate::invalid(
                    "DE graphic",
                    at,
                    "ambiguous graphic identity",
                ));
            }
            continue;
        }
        found = Some(graphic);
    }
    found.ok_or_else(|| crate::invalid("DE graphic", 0, format!("missing {file}")))
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
