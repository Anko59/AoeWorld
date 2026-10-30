//! Bounded common DE unit animation fields, shared by every spawned unit kind.
use super::reader::Reader;
use crate::{Error, invalid};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
mod tasks;

#[derive(Clone, Debug, Serialize)]
pub struct Unit {
    pub id: i16,
    pub name: String,
    pub standing: i16,
    pub dying: i16,
    pub dead: i16,
    pub walking: i16,
    pub running: i16,
    pub attack: i16,
    pub work: i16,
    pub carrying: i16,
    pub release: i16,
}
fn parse(bytes: &[u8], at: usize, id: i16) -> Result<Unit, Error> {
    let mut r = Reader::new(bytes, at);
    let kind = r.u8()?;
    if !matches!(kind, 10 | 20 | 25 | 30 | 40 | 50 | 60 | 70 | 80) || r.i16()? != id {
        return Err(r.fail("unit identity"));
    }
    r.skip(10)?;
    let standing = r.i16()?;
    r.skip(2)?;
    let dying = r.i16()?;
    r.pos = at + 45;
    let dead = r.i16()?;
    r.pos = at + 157;
    let damage = usize::from(r.u8()?);
    r.bounded(damage, 32, "damage graphics")?;
    r.skip(damage * 5 + 22)?;
    if r.u16()? != 0x0a60 {
        return Err(r.fail("unit name marker"));
    }
    let n = usize::from(r.u16()?);
    r.bounded(n, 256, "unit name")?;
    let name = std::str::from_utf8(r.take(n)?)
        .map_err(|_| invalid("DE unit", at, "unit name"))?
        .to_owned();
    if name.is_empty() || r.i16()? != id || r.i16()? < 0 {
        return Err(r.fail("unit copy identity"));
    }
    let mut unit = Unit {
        id,
        name,
        standing,
        dying,
        dead,
        walking: -1,
        running: -1,
        attack: -1,
        work: -1,
        carrying: -1,
        release: 0,
    };
    if kind < 30 {
        return Ok(unit);
    }
    r.skip(4)?;
    unit.walking = r.i16()?;
    unit.running = r.i16()?;
    r.skip(37)?;
    if kind < 40 {
        return Ok(unit);
    }
    r.skip(10)?;
    let drops = usize::from(r.u16()?);
    r.bounded(drops, 64, "drop sites")?;
    r.skip(drops * 2 + 14)?;
    let tasks = usize::from(r.u16()?);
    r.bounded(tasks, 256, "unit tasks")?;
    for _ in 0..tasks {
        let task = r.take(69)?;
        let graphic = |at| i16::from_le_bytes([task[at], task[at + 1]]);
        if unit.work < 0 {
            unit.work = if graphic(51) >= 0 {
                graphic(51)
            } else {
                graphic(49)
            };
        }
        if unit.carrying < 0 {
            unit.carrying = graphic(53);
        }
    }
    if kind < 50 {
        return Ok(unit);
    }
    r.skip(2)?;
    for _ in 0..2 {
        let n = usize::from(r.u16()?);
        r.bounded(n, 64, "combat entries")?;
        r.skip(n * 4)?;
    }
    r.skip(6 + 12 + 2 + 3)?;
    unit.release = r.i16()?;
    r.skip(13 + 4 + 4)?;
    unit.attack = r.i16()?;
    Ok(unit)
}
/// First validated civilization record, plus every referenced corpse record.
pub fn find(bytes: &[u8], ids: &[i16]) -> Result<BTreeMap<i16, Unit>, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0") || ids.len() > 256 {
        return Err(invalid("DE unit", 0, "unsupported selection"));
    }
    let mut wanted: BTreeSet<_> = ids.iter().copied().collect();
    let mut found = BTreeMap::new();
    for _ in 0..2 {
        let mut remaining = wanted.iter().filter(|id| !found.contains_key(*id)).count();
        if remaining == 0 {
            break;
        }
        for (at, head) in bytes.windows(3).enumerate() {
            if !matches!(head[0], 10 | 20 | 25 | 30 | 40 | 50 | 60 | 70 | 80) {
                continue;
            }
            let id = i16::from_le_bytes([head[1], head[2]]);
            if wanted.contains(&id)
                && !found.contains_key(&id)
                && let Ok(unit) = parse(bytes, at, id)
            {
                found.insert(id, unit);
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
            }
        }
        for unit in found.values() {
            if unit.dead >= 0 {
                wanted.insert(unit.dead);
            }
        }
        if wanted.len() > 512 {
            return Err(invalid("DE unit", 0, "corpse references exceed bound"));
        }
    }
    let shared = tasks::find(
        bytes,
        ids.iter().copied().max().unwrap_or(0).max(0) as usize + 1,
    )?;
    for unit in found.values_mut() {
        if let Some(&(work, carry)) = shared.get(&unit.id) {
            if unit.work < 0 {
                unit.work = work;
            }
            if unit.carrying < 0 {
                unit.carrying = carry;
            }
        }
    }
    for id in ids {
        if !found.contains_key(id) {
            return Err(invalid("DE unit", 0, format!("missing unit {id}")));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
