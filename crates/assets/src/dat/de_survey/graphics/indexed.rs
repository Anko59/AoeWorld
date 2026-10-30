//! Graphic identities and drawable DAT delta trees, with bounded recursion.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub struct Part {
    pub animation: Animation,
    pub graphic: i16,
    pub x: i32,
    pub y: i32,
    pub layer: u8,
}
#[derive(Clone, Debug)]
pub struct Graphic {
    pub parts: Vec<Part>,
}
struct Raw {
    own: Option<Part>,
    deltas: Vec<(i16, i16, i16)>,
}
fn raw(bytes: &[u8], at: usize) -> Result<(i16, Raw), Error> {
    let mut r = Reader::new(bytes, at);
    let file = debug(&mut r)?;
    let particle = debug(&mut r)?;
    let sprite = r.i32()?;
    r.skip(2)?;
    let layer = r.u8()?;
    r.skip(11)?;
    let n = usize::from(r.u16()?);
    r.bounded(n, 256, "graphic deltas")?;
    r.skip(6)?;
    let sounds = r.u8()?;
    let frames = r.u16()?;
    let angles = r.u16()?;
    r.skip(4)?;
    let seconds = r.f32()?;
    let replay_delay = r.f32()?;
    r.skip(1)?;
    let id = r.i16()?;
    r.skip(2)?;
    if id < 0
        || sounds > 1
        || frames > 255
        || angles > 64
        || !seconds.is_finite()
        || !(0.0..=60.0).contains(&seconds)
        || !replay_delay.is_finite()
        || replay_delay < 0.0
    {
        return Err(r.fail("graphic metadata"));
    }
    let mut deltas = Vec::new();
    for _ in 0..n {
        let target = r.i16()?;
        r.skip(6)?;
        let x = r.i16()?;
        let y = r.i16()?;
        let angle = r.i16()?;
        r.skip(2)?;
        if target >= 0 {
            if angle != -1 {
                return Err(r.fail("angle-restricted graphic delta"));
            }
            deltas.push((target, x, y));
        }
    }
    let no_pixels = file == "None"
        || !particle.is_empty()
        || (!file.is_empty()
            && file
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'));
    let own = ((file.ends_with("_x1") || file.ends_with("_x2")) && frames > 0 && angles > 0)
        .then_some(Part {
            graphic: id,
            animation: Animation {
                file,
                sprite,
                frames,
                angles,
                seconds,
                replay_delay,
            },
            x: 0,
            y: 0,
            layer,
        });
    if own.is_none() && deltas.is_empty() && !no_pixels {
        return Err(r.fail("empty graphic"));
    }
    Ok((id, Raw { own, deltas }))
}
fn resolve(
    id: i16,
    raw: &BTreeMap<i16, Raw>,
    seen: &mut BTreeSet<i16>,
    x: i32,
    y: i32,
    out: &mut Vec<Part>,
) -> Result<(), Error> {
    if seen.len() >= 32 || !seen.insert(id) || out.len() >= 32 {
        return Err(crate::invalid(
            "DE graphic",
            0,
            "cyclic or oversized delta tree",
        ));
    }
    // Non-drawable particle-only deltas belong to the effects pass.
    if let Some(r) = raw.get(&id) {
        if let Some(own) = &r.own {
            let mut p = own.clone();
            p.x += x;
            p.y += y;
            out.push(p);
        }
        for &(child, dx, dy) in &r.deltas {
            resolve(child, raw, seen, x + i32::from(dx), y + i32::from(dy), out)?;
        }
    } else {
        return Err(crate::invalid("DE graphic", 0, "missing delta graphic"));
    }
    seen.remove(&id);
    Ok(())
}
pub fn indexed(bytes: &[u8]) -> Result<BTreeMap<i16, Graphic>, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0") {
        return Err(crate::invalid("DE graphic", 0, "unsupported version"));
    }
    let mut all = BTreeMap::new();
    for (at, head) in bytes.windows(4).enumerate() {
        if head[..2] != [0x60, 0x0a] {
            continue;
        }
        let n = usize::from(u16::from_le_bytes([head[2], head[3]]));
        if n > 256 {
            continue;
        }
        let Some(name) = bytes
            .get(at + 4..at + 4 + n)
            .and_then(|s| std::str::from_utf8(s).ok())
        else {
            continue;
        };
        if !name.is_ascii() {
            continue;
        }
        if let Ok((id, r)) = raw(bytes, at) {
            all.entry(id).or_insert(r);
        }
        if all.len() > 32768 {
            return Err(crate::invalid("DE graphic", at, "graphic inventory bound"));
        }
    }
    let mut result = BTreeMap::new();
    for &id in all.keys() {
        let mut parts = Vec::new();
        if resolve(id, &all, &mut BTreeSet::new(), 0, 0, &mut parts).is_ok() {
            parts.sort_by_key(|p| p.layer);
            result.insert(id, Graphic { parts });
        }
    }
    Ok(result)
}
