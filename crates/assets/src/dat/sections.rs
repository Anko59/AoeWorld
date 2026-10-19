//! The leading `empires2.dat` sections: version, terrain restrictions, player
//! colours, sounds and the graphics table. Sizes were measured on the trial's
//! `VER 5.7` data; an unlisted version is rejected rather than guessed.
use super::reader::{Reader, text};
use super::{Graphic, SUPPORTED_VERSIONS};
use crate::Error;

const MAX_TERRAINS: usize = 512;
const MAX_RESTRICTIONS: usize = 256;
const MAX_COLOURS: usize = 64;
const MAX_SOUNDS: usize = 8_192;
const MAX_SOUND_FILES: usize = 64;
const MAX_GRAPHICS: usize = 32_768;
const MAX_DELTAS: usize = 256;
const MAX_ATTACK_ANGLES: usize = 64;
const GRAPHIC_BYTES: usize = 78;
const DELTA_BYTES: usize = 16;
const SOUND_FILE_BYTES: usize = 23;
const COLOUR_BYTES: usize = 36;
/// One restriction: a float and a 16-byte pass-graphic record per terrain.
const RESTRICTION_PER_TERRAIN: usize = 20;

pub(super) fn version(r: &mut Reader) -> Result<String, Error> {
    let version = text(r.take(8)?);
    if !SUPPORTED_VERSIONS.contains(&version.as_str()) {
        return Err(Error::Unsupported {
            format: super::reader::FORMAT,
            detail: format!("version {version:?} (measured: {SUPPORTED_VERSIONS:?})"),
        });
    }
    Ok(version)
}

/// Skips restrictions, colours and sounds; returns how many sounds were seen.
pub(super) fn skip_to_graphics(r: &mut Reader) -> Result<usize, Error> {
    let restrictions = usize::from(r.u16()?);
    let terrains = usize::from(r.u16()?);
    r.bounded(restrictions, MAX_RESTRICTIONS, "terrain restrictions")?;
    r.bounded(terrains, MAX_TERRAINS, "terrains")?;
    // Two pointer arrays precede the restrictions themselves.
    r.skip(restrictions * 8)?;
    r.skip(restrictions * terrains * RESTRICTION_PER_TERRAIN)?;
    let colours = usize::from(r.u16()?);
    r.bounded(colours, MAX_COLOURS, "player colours")?;
    r.skip(colours * COLOUR_BYTES)?;
    let sounds = usize::from(r.u16()?);
    r.bounded(sounds, MAX_SOUNDS, "sounds")?;
    for _ in 0..sounds {
        r.skip(4)?;
        let files = usize::from(r.u16()?);
        r.bounded(files, MAX_SOUND_FILES, "sound files")?;
        r.skip(4)?;
        r.skip(files * SOUND_FILE_BYTES)?;
    }
    Ok(sounds)
}

pub(super) fn graphics(r: &mut Reader) -> Result<Vec<Graphic>, Error> {
    let slots = usize::from(r.u16()?);
    r.bounded(slots, MAX_GRAPHICS, "graphic slots")?;
    let mut present = Vec::with_capacity(slots);
    for _ in 0..slots {
        present.push(r.i32()? != 0);
    }
    let mut out = Vec::new();
    for (slot, here) in present.into_iter().enumerate() {
        if here {
            let graphic = graphic(r)?;
            if usize::try_from(graphic.id).ok() != Some(slot) {
                return Err(r.fail(format!("graphic slot {slot} carries id {}", graphic.id)));
            }
            out.push(graphic);
        }
    }
    Ok(out)
}

fn graphic(r: &mut Reader) -> Result<Graphic, Error> {
    let start = r.pos;
    let name = text(r.take(21)?);
    let file_name = text(r.take(13)?);
    let slp_id = r.i32()?;
    r.skip(3)?; // loaded, old colour flag, layer
    r.skip(2)?; // player colour
    r.skip(1 + 8)?; // transparent selection, selection rectangle
    let deltas = usize::from(r.u16()?);
    r.bounded(deltas, MAX_DELTAS, "graphic deltas")?;
    r.skip(2)?; // sound
    let attack_sounds = r.u8()? != 0;
    let frame_count = r.u16()?;
    let angle_count = r.u16()?;
    r.skip(4)?; // new speed
    let frame_rate = r.f32()?;
    let replay_delay = r.f32()?;
    r.skip(1)?; // sequence type
    let id = i32::from(r.i16()?);
    r.skip(2)?; // mirroring mode, editor flag
    if r.pos - start != GRAPHIC_BYTES {
        return Err(r.fail("graphic record size drifted"));
    }
    let mut delta_graphics = Vec::with_capacity(deltas);
    for _ in 0..deltas {
        delta_graphics.push(r.i16()?);
        r.skip(DELTA_BYTES - 2)?;
    }
    if attack_sounds {
        let angles = r.bounded(usize::from(angle_count), MAX_ATTACK_ANGLES, "attack angles")?;
        r.skip(angles * 12)?;
    }
    Ok(Graphic {
        id,
        name,
        file_name,
        slp_id,
        frame_count,
        angle_count,
        frame_rate,
        replay_delay,
        delta_graphics,
    })
}
