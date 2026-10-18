//! Civilisation anchor, unit headers (tasks) and the unit records.
use super::reader::{Reader, text};
use super::{Cost, Report, Task, Unit};
use crate::Error;

const MAX_CIVILISATIONS: usize = 64;
const MAX_RESOURCES: usize = 1_024;
const MAX_UNIT_SLOTS: usize = 4_096;
const MAX_NAME: usize = 128;
const MAX_DAMAGE_SPRITES: usize = 32;
const MAX_TASKS: usize = 256;
const MAX_COMBAT_ENTRIES: usize = 64;
const TASK_BYTES: usize = 59;
/// Fixed bytes of a unit before the damage-sprite count.
const DAMAGE_COUNT_AT: usize = 145;
const DAMAGE_SPRITE_BYTES: usize = 5;
const COMBAT_TAIL: usize = 56;
const CREATABLE_BYTES: usize = 65;
const BUILDING_BYTES: usize = 80;

struct Anchor {
    offset: usize,
    count: usize,
    name: String,
    slots: usize,
    pointers: Vec<bool>,
    units_at: usize,
}

pub(super) fn decode(bytes: &[u8], from: usize, report: &mut Report) -> Result<Vec<Unit>, Error> {
    let anchor = find_civilisations(bytes, from).ok_or_else(|| {
        crate::invalid(super::reader::FORMAT, from, "no civilisation block found")
    })?;
    report.civilisation_offset = Some(anchor.offset);
    report.civilisation_count = Some(anchor.count);
    report.civilisation_name = Some(anchor.name.clone());
    report.unit_slots = anchor.slots;
    let tasks = find_headers(bytes, from, &anchor);
    report.unit_headers_offset = tasks.as_ref().map(|found| found.0);
    let mut tasks = tasks.map(|found| found.1).unwrap_or_default();
    let mut r = Reader::new(bytes, anchor.units_at);
    let mut units = Vec::new();
    for (slot, here) in anchor.pointers.iter().enumerate() {
        if !*here {
            continue;
        }
        let at = r.pos;
        let mut unit = unit(&mut r)?;
        if usize::try_from(unit.id).ok() != Some(slot) {
            return Err(crate::invalid(
                super::reader::FORMAT,
                at,
                format!("unit slot {slot} carries id {}", unit.id),
            ));
        }
        unit.tasks = tasks.get_mut(slot).map(std::mem::take).unwrap_or_default();
        units.push(unit);
        report.units_parsed = units.len();
    }
    Ok(units)
}

fn find_civilisations(bytes: &[u8], from: usize) -> Option<Anchor> {
    (from..bytes.len()).find_map(|at| civilisation_at(bytes, at))
}

fn civilisation_at(bytes: &[u8], at: usize) -> Option<Anchor> {
    let head = bytes.get(at..at + 25)?;
    let count = usize::from(u16::from_le_bytes([head[0], head[1]]));
    if !(1..=MAX_CIVILISATIONS).contains(&count) || head[2] != 1 {
        return None;
    }
    let name = &head[3..23];
    let len = name.iter().position(|b| *b == 0)?;
    let printable = name[..len].iter().all(|b| (0x20..0x7f).contains(b));
    if len == 0 || !printable || name[len..].iter().any(|b| *b != 0) {
        return None;
    }
    let resources = usize::from(u16::from_le_bytes([head[23], head[24]]));
    if !(1..=MAX_RESOURCES).contains(&resources) {
        return None;
    }
    let mut r = Reader::new(bytes, at + 25 + 4 + 4 * resources + 1);
    let slots = usize::from(r.u16().ok()?);
    if !(1..=MAX_UNIT_SLOTS).contains(&slots) {
        return None;
    }
    let mut pointers = Vec::with_capacity(slots);
    for _ in 0..slots {
        pointers.push(r.i32().ok()? != 0);
    }
    if !pointers.iter().any(|here| *here) {
        return None;
    }
    Some(Anchor {
        offset: at,
        count,
        name: text(name),
        slots,
        pointers,
        units_at: r.pos,
    })
}

/// Header sections are variable length, so the start is found as the one
/// offset whose walk ends exactly at the civilisation anchor.
fn find_headers(bytes: &[u8], from: usize, anchor: &Anchor) -> Option<(usize, Vec<Vec<Task>>)> {
    let wanted = u32::try_from(anchor.slots).ok()?.to_le_bytes();
    (from..anchor.offset.checked_sub(4)?)
        .filter(|at| bytes.get(*at..*at + 4) == Some(&wanted))
        .find_map(|at| {
            let (end, tasks) = walk_headers(bytes, at, anchor.slots).ok()?;
            (end == anchor.offset).then_some((at, tasks))
        })
}

fn walk_headers(bytes: &[u8], at: usize, slots: usize) -> Result<(usize, Vec<Vec<Task>>), Error> {
    let mut r = Reader::new(bytes, at + 4);
    let mut all = Vec::with_capacity(slots);
    for _ in 0..slots {
        match r.u8()? {
            0 => all.push(Vec::new()),
            1 => {
                let count = usize::from(r.u16()?);
                r.bounded(count, MAX_TASKS, "tasks")?;
                let mut list = Vec::with_capacity(count);
                for _ in 0..count {
                    list.push(task(&mut r)?);
                }
                all.push(list);
            }
            other => return Err(r.fail(format!("unit header flag {other}"))),
        }
    }
    Ok((r.pos, all))
}

fn task(r: &mut Reader) -> Result<Task, Error> {
    let start = r.pos;
    let kind = r.i16()?;
    let id = r.i16()?;
    r.skip(1)?; // default flag
    let action = r.i16()?;
    let class = r.i16()?;
    let unit = r.i16()?;
    let terrain = r.i16()?;
    let resource_in = r.i16()?;
    r.skip(2)?; // multiplier
    let resource_out = r.i16()?;
    r.skip(2)?; // unused resource
    r.skip(26)?; // work values, range, targeting flags, gather type
    let mut graphics = [0i16; 4];
    for slot in &mut graphics {
        *slot = r.i16()?;
    }
    r.skip(4)?; // sounds
    debug_assert_eq!(r.pos - start, TASK_BYTES);
    Ok(Task {
        kind,
        id,
        action,
        class,
        unit,
        terrain,
        resource_in,
        resource_out,
        graphics,
    })
}

fn pairs(r: &mut Reader) -> Result<Vec<(i16, i16)>, Error> {
    let count = usize::from(r.u16()?);
    r.bounded(count, MAX_COMBAT_ENTRIES, "attack or armour entries")?;
    (0..count).map(|_| Ok((r.i16()?, r.i16()?))).collect()
}

fn unit(r: &mut Reader) -> Result<Unit, Error> {
    let start = r.pos;
    let kind = r.u8()?;
    if ![10, 20, 25, 30, 40, 50, 60, 70, 80].contains(&kind) {
        return Err(r.fail(format!("unsupported unit type {kind}")));
    }
    let name_len = usize::from(r.u16()?);
    r.bounded(name_len, MAX_NAME, "unit name length")?;
    let id = i32::from(r.i16()?);
    r.skip(6)?; // language strings, class
    let standing = [r.i16()?, r.i16()?];
    let dying = r.i16()?;
    r.skip(3)?; // undead graphic, death mode
    let hit_points = r.i16()?;
    let line_of_sight = r.f32()?;
    r.pos = start + DAMAGE_COUNT_AT;
    let sprites = usize::from(r.u8()?);
    r.bounded(sprites, MAX_DAMAGE_SPRITES, "damage sprites")?;
    r.skip(sprites * DAMAGE_SPRITE_BYTES + 6)?; // sprites, selection and dying sounds, flags
    let name = text(r.take(name_len)?);
    r.skip(4)?; // repeated ids
    let mut out = Unit {
        id,
        kind,
        name,
        hit_points,
        line_of_sight,
        standing,
        dying,
        speed: None,
        walking: None,
        attack_graphic: None,
        attacks: Vec::new(),
        armours: Vec::new(),
        costs: Vec::new(),
        train_time: None,
        train_location: None,
        construction_graphic: None,
        snow_graphic: None,
        tasks: Vec::new(),
    };
    if kind >= 20 {
        out.speed = Some(r.f32()?);
    }
    if kind >= 30 {
        out.walking = Some([r.i16()?, r.i16()?]);
        r.skip(33)?; // rotation, tracking, turn angles
    }
    if kind >= 40 {
        r.skip(20)?; // search radius, work rate, drop sites, sounds
    }
    if kind >= 50 {
        r.skip(1)?; // default armour
        out.attacks = pairs(r)?;
        out.armours = pairs(r)?;
        r.skip(COMBAT_TAIL - 14)?;
        out.attack_graphic = Some(r.i16()?);
        r.skip(12)?;
    }
    if kind == 60 {
        r.skip(9)?;
    }
    if kind >= 70 {
        for _ in 0..3 {
            out.costs.push(Cost {
                resource: r.i16()?,
                amount: r.i16()?,
                enabled: r.i16()?,
            });
        }
        out.train_time = Some(r.i16()?);
        out.train_location = Some(r.i16()?);
        r.skip(CREATABLE_BYTES - 22)?;
    }
    if kind == 80 {
        out.construction_graphic = Some(r.i16()?);
        out.snow_graphic = Some(r.i16()?);
        r.skip(BUILDING_BYTES - 4)?;
    }
    Ok(out)
}
