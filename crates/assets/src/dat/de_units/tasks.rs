//! Shared unit-header tasks end exactly at the validated civilization anchor.
use super::*;
type Tasks = BTreeMap<i16, (i16, i16)>;
fn anchor(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    if at < 3 || bytes.get(at - 1) != Some(&1) {
        return None;
    }
    let civs = u16::from_le_bytes(bytes.get(at - 3..at - 1)?.try_into().ok()?);
    if !(1..=128).contains(&civs) {
        return None;
    }
    let mut r = Reader::new(bytes, at);
    if r.u16().ok()? != 0x0a60 {
        return None;
    }
    let n = usize::from(r.u16().ok()?);
    if !(1..=32).contains(&n) {
        return None;
    }
    if !r.take(n).ok()?.iter().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    let resources = usize::from(r.u16().ok()?);
    if !(1..=2048).contains(&resources) {
        return None;
    }
    r.skip(4 + resources * 4 + 1).ok()?;
    let slots = usize::from(r.u16().ok()?);
    if !(1..=32768).contains(&slots) {
        return None;
    }
    for _ in 0..slots {
        if !matches!(r.i32().ok()?, 0 | 1) {
            return None;
        }
    }
    Some((at - 3, slots))
}
fn walk(bytes: &[u8], at: usize, slots: usize) -> Result<(usize, Tasks), Error> {
    let mut r = Reader::new(bytes, at + 4);
    let mut out = BTreeMap::new();
    for id in 0..slots {
        match r.u8()? {
            0 => {}
            1 => {
                let n = usize::from(r.u16()?);
                r.bounded(n, 256, "shared tasks")?;
                let (mut work, mut carry) = (-1, -1);
                for _ in 0..n {
                    let task = r.take(69)?;
                    let value = |n| i16::from_le_bytes([task[n], task[n + 1]]);
                    if work < 0 {
                        work = if value(51) >= 0 { value(51) } else { value(49) };
                    }
                    if carry < 0 {
                        carry = value(53);
                    }
                }
                out.insert(id as i16, (work, carry));
            }
            _ => return Err(r.fail("shared task header")),
        }
    }
    Ok((r.pos, out))
}
pub(super) fn find(bytes: &[u8], minimum: usize) -> Result<Tasks, Error> {
    for (at, head) in bytes.windows(2).enumerate() {
        if head != [0x60, 0x0a] {
            continue;
        }
        let Some((end, slots)) = anchor(bytes, at) else {
            continue;
        };
        for (start, head) in bytes[..end].windows(4).enumerate().rev() {
            let count = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
            if !(minimum..=32768).contains(&count) || count > slots {
                continue;
            }
            if let Ok((next, tasks)) = walk(&bytes[..end], start, count)
                && next == end
            {
                return Ok(tasks);
            }
        }
    }
    Ok(BTreeMap::new())
}
