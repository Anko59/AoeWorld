//! Focused numeric inspection of the owner's DE VER 8.9 dat, not a full decoder.
//! Layout: SiegeEngineers/genieutils-py (unit.py, tech.py, effect.py).
//! Records are located by typed identifiers and validated debug names. No
//! offsets, numeric gameplay values or original data are embedded in this tool.
use super::reader::Reader;
use crate::Error;
use serde::Serialize;
#[path = "de_military.rs"]
mod military;
pub use military::ProjectileSource;
#[path = "de_survey/economy.rs"]
mod economy;
pub mod graphics;
mod soldiers;
pub use soldiers::soldier;

const BUILDINGS: &[(i16, i32, &str)] = &[
    (82, 5142, "CSTL"),
    (104, 5138, "CRCH"),
    (209, 5176, "UNIV"),
    (49, 5169, "SIWS"),
    (87, 5128, "ARRG"),
    (101, 5171, "STBL"),
    (103, 5131, "BLAC"),
    (84, 5161, "MRKT"),
    (79, 5178, "WCTW"),
    (792, 5186, "PGTAX"),
    (71, 5164, ""),
    (105, 5131, ""),
    (129, 5157, ""),
    (463, 5344, ""),
    (498, 5135, ""),
    (563, 5464, ""),
    (585, 5487, ""),
    (12, 5135, ""),
    (70, 5344, ""),
    (68, 5157, ""),
    (562, 5464, ""),
    (584, 5487, ""),
    (50, 5149, ""),
    (72, 5202, ""),
    (598, 5504, ""),
];
const TECHS: &[(i16, i32)] = &[
    (102, 7102),
    (207, 7204),
    (197, 7176),
    (100, 7100),
    (98, 7098),
    (254, 7261),
    (249, 7246),
    (280, 7282),
    (68, 7068),
    (76, 7076),
    (82, 7082),
    (200, 7150),
    (212, 7209),
    (13, 7013),
    (203, 7190),
    (182, 7180),
    (50, 7050),
    (93, 7093),
    (321, 7321),
    (202, 7189),
    (14, 7014),
    (55, 7055),
    (278, 7276),
    (213, 7211),
    (101, 7101),
    (8, 7008),
    (222, 7230),
    (67, 7067),
    (74, 7074),
    (81, 7081),
    (199, 7172),
    (211, 7208),
];
const SOLDIERS: &[(i16, i32)] = &[
    (77, 5081),
    (358, 5408),
    (24, 5084),
    (6, 5087),
    (38, 5068),
    (546, 5444),
    (125, 5099),
    (280, 5095),
    (35, 5094),
    (4, 5083),
    (7, 5088),
    (93, 5078),
    (74, 5079),
    (75, 5080),
    (448, 5326),
];

#[derive(Debug, Serialize)]
pub struct Building {
    pub id: i16,
    pub offset: usize,
    pub name: String,
    pub hp: i16,
    pub los: f32,
    pub collision: [f32; 2],
    pub graphic: i16,
    pub speed: f32,
    pub work_rate: f32,
    pub resource_capacity: i16,
    pub projectile: i16,
    pub frame_delay: i16,
    pub attacks: Vec<(i16, i16)>,
    pub armours: Vec<(i16, i16)>,
    pub range: f32,
    pub blast_width: f32,
    pub min_range: f32,
    pub reload: f32,
    pub cost: Vec<(i16, i16, i16)>,
    pub resource_storage: Vec<(i16, f32, u8)>,
    pub build_seconds: i16,
    /// Builder location, DE grid button index and hotkey action id.
    pub build_location: i16,
    pub button: u8,
    pub hot_key: i32,
}
#[derive(Debug, Serialize)]
pub struct Technology {
    pub id: i16,
    pub offset: usize,
    pub name: String,
    pub requires: Vec<i16>,
    pub cost: Vec<(i16, i16, u8)>,
    pub effect: i16,
    pub locations: Vec<(i16, i16, u8, i32)>,
}
#[derive(Debug, Serialize)]
pub struct Effect {
    pub name: String,
    pub commands: Vec<(u8, i16, i16, i16, f32)>,
}
#[derive(Debug, Serialize)]
pub struct Survey {
    pub version: String,
    pub source_hash: String,
    pub inflated_bytes: usize,
    pub buildings: Vec<Building>,
    pub soldiers: Vec<Building>,
    pub workers: Vec<Building>,
    pub projectiles: Vec<ProjectileSource>,
    pub animations: Vec<graphics::Animation>,
    pub technologies: Vec<Technology>,
    pub effects: Vec<(i16, Effect)>,
}

fn count(r: &mut Reader, max: usize, label: &str) -> Result<usize, Error> {
    let n = usize::from(r.u16()?);
    r.bounded(n, max, label)
}
fn debug(r: &mut Reader) -> Result<String, Error> {
    if r.u16()? != 0x0a60 {
        return Err(r.fail("missing DE debug string marker"));
    }
    let n = count(r, 256, "debug name")?;
    let bytes = r.take(n)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| r.fail("invalid UTF-8 debug name"))
}
fn pairs(r: &mut Reader) -> Result<Vec<(i16, i16)>, Error> {
    let n = count(r, 64, "combat entries")?;
    (0..n).map(|_| Ok((r.i16()?, r.i16()?))).collect()
}

pub(super) fn unit_record(bytes: &[u8], at: usize, id: i16, name: &str) -> Result<Building, Error> {
    let mut r = Reader::new(bytes, at);
    let unit_type = r.u8()?;
    if !matches!(unit_type, 70 | 80) || r.i16()? != id {
        return Err(r.fail("wrong building identity"));
    }
    r.skip(10)?;
    let graphic = r.i16()?;
    r.skip(7)?;
    let hp = r.i16()?;
    let los = r.f32()?;
    r.skip(1)?;
    let collision = [r.f32()?, r.f32()?];
    if hp <= 0 || !los.is_finite() || collision.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err(r.fail("invalid building dimensions or stats"));
    }
    r.pos = at + 79;
    let resource_capacity = r.i16()?;
    r.pos = at + 136;
    let resource_storage = (0..3)
        .map(|_| Ok((r.i16()?, r.f32()?, r.u8()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let n = usize::from(r.u8()?);
    r.bounded(n, 32, "damage graphics")?;
    r.skip(n * 5 + 22)?;
    let actual_name = debug(&mut r)?;
    if !name.is_empty() && actual_name != name {
        return Err(r.fail("building name disagrees with identity"));
    }
    if r.i16()? != id || r.i16()? < 0 {
        return Err(r.fail("building copy/base id disagrees"));
    }
    let speed = r.f32()?;
    if !speed.is_finite() || speed < 0. {
        return Err(r.fail("invalid speed"));
    }
    r.skip(41 + 6)?; // dead-fish, bird's default task and search radius
    let work_rate = r.f32()?;
    if !work_rate.is_finite() || work_rate < 0. {
        return Err(r.fail("invalid work rate"));
    }
    let drops = count(&mut r, 64, "drop sites")?;
    r.skip(drops * 2 + 14)?;
    let tasks = count(&mut r, 256, "tasks")?;
    r.skip(tasks * 69)?;
    r.skip(2)?; // default armour
    let attacks = pairs(&mut r)?;
    let armours = pairs(&mut r)?;
    r.skip(6)?;
    let range = r.f32()?;
    let blast_width = r.f32()?;
    let reload = r.f32()?;
    let projectile = r.i16()?;
    r.skip(3)?; // accuracy and break-off-combat flag
    let frame_delay = r.i16()?;
    r.skip(13)?; // projectile displacement and blast attack level
    let min_range = r.f32()?;
    r.skip(38)?;
    let cost = (0..3)
        .map(|_| Ok((r.i16()?, r.i16()?, r.i16()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let n = count(&mut r, 16, "training locations")?;
    let mut build_seconds = 0;
    let (mut build_location, mut button, mut hot_key) = (0, 0, 0);
    for i in 0..n {
        let seconds = r.i16()?;
        let location = r.i16()?;
        let slot = r.u8()?;
        let key = r.i32()?;
        if i == 0 {
            build_seconds = seconds;
            (build_location, button, hot_key) = (location, slot, key);
        }
    }
    // Profession variants are not trained directly and have no train location.
    if build_seconds < 0
        || (unit_type == 80 && build_seconds == 0)
        || [range, reload, min_range, blast_width]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(r.fail("invalid build or combat timing"));
    }
    Ok(Building {
        id,
        offset: at,
        name: actual_name,
        hp,
        los,
        collision,
        graphic,
        speed,
        work_rate,
        resource_capacity,
        projectile,
        frame_delay,
        attacks,
        armours,
        range,
        blast_width,
        min_range,
        reload,
        cost,
        resource_storage,
        build_seconds,
        build_location,
        button,
        hot_key,
    })
}

fn technology(bytes: &[u8], id: i16, language: i32) -> Result<Technology, Error> {
    let mut signature = language.to_le_bytes().to_vec();
    signature.extend_from_slice(&(language + 1000).to_le_bytes());
    for at in bytes
        .windows(8)
        .enumerate()
        .filter_map(|(at, b)| (b == signature).then_some(at))
    {
        let Some(start) = at.checked_sub(33) else {
            continue;
        };
        let parsed = (|| {
            let mut r = Reader::new(bytes, start);
            let requires = (0..6).map(|_| r.i16()).collect::<Result<Vec<_>, _>>()?;
            let cost = (0..3)
                .map(|_| Ok((r.i16()?, r.i16()?, r.u8()?)))
                .collect::<Result<Vec<_>, Error>>()?;
            r.skip(6 + 8)?;
            let effect = r.i16()?;
            r.skip(12)?;
            let name = debug(&mut r)?;
            r.skip(1)?;
            let n = count(&mut r, 16, "research locations")?;
            let locations = (0..n)
                .map(|_| Ok((r.i16()?, r.i16()?, r.u8()?, r.i32()?)))
                .collect::<Result<Vec<_>, Error>>()?;
            if locations.is_empty() || effect < 0 {
                return Err(r.fail("invalid research record"));
            }
            Ok(Technology {
                id,
                offset: start,
                name,
                requires,
                cost,
                effect,
                locations,
            })
        })();
        if parsed.is_ok() {
            return parsed;
        }
    }
    Err(crate::invalid(
        "DE survey",
        0,
        format!("technology {id} not found"),
    ))
}

fn effect(bytes: &[u8], at: usize) -> Result<(Effect, usize), Error> {
    let mut r = Reader::new(bytes, at);
    let name = debug(&mut r)?;
    let n = count(&mut r, 4096, "effect commands")?;
    let commands = (0..n)
        .map(|_| Ok((r.u8()?, r.i16()?, r.i16()?, r.i16()?, r.f32()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    if commands.iter().any(|c| !c.4.is_finite()) {
        return Err(r.fail("non-finite effect"));
    }
    Ok((Effect { name, commands }, r.pos))
}

pub fn survey(bytes: &[u8]) -> Result<Survey, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0") {
        return Err(crate::invalid(
            "DE survey",
            0,
            "only measured VER 8.9 is supported",
        ));
    }
    let mut buildings = Vec::new();
    for &(id, language, name) in BUILDINGS {
        let mut signature = vec![80];
        signature.extend_from_slice(&id.to_le_bytes());
        signature.extend_from_slice(&language.to_le_bytes());
        let at = bytes
            .windows(signature.len())
            .enumerate()
            .find_map(|(at, b)| (b == signature).then_some(at))
            .ok_or_else(|| crate::invalid("DE survey", 0, format!("building {id} not found")))?;
        let building = unit_record(bytes, at, id, name)?;
        buildings.push(building);
    }
    let technologies = TECHS
        .iter()
        .map(|&(id, lang)| technology(bytes, id, lang))
        .collect::<Result<Vec<_>, _>>()?;
    let soldiers = SOLDIERS
        .iter()
        .map(|&(id, _)| soldier(bytes, id))
        .collect::<Result<Vec<_>, Error>>()?;
    // Effects are a contiguous debug-name/count/command table. Locate the
    // longest structurally valid chain before the first unit, then require its
    // preceding count to agree exactly. Graphic debug strings cannot qualify.
    let end = buildings
        .iter()
        .map(|b| b.offset)
        .min()
        .unwrap_or(bytes.len());
    let mut links = std::collections::BTreeMap::new();
    for (at, w) in bytes[..end].windows(2).enumerate() {
        if w == [0x60, 0x0a]
            && let Ok((_, next)) = effect(bytes, at)
        {
            links.insert(at, next);
        }
    }
    let mut effects = None;
    for &start in links.keys() {
        if start < 4 {
            continue;
        }
        let total = i32::from_le_bytes([
            bytes[start - 4],
            bytes[start - 3],
            bytes[start - 2],
            bytes[start - 1],
        ]);
        if !(256..=8192).contains(&total) {
            continue;
        }
        let mut cursor = start;
        let mut rows = Vec::new();
        for _ in 0..total {
            let Some(&next) = links.get(&cursor) else {
                break;
            };
            rows.push(cursor);
            cursor = next;
        }
        if rows.len() == total as usize {
            effects = Some(rows);
            break;
        }
    }
    let rows = effects
        .ok_or_else(|| crate::invalid("DE survey", 0, "validated effect table not found"))?;
    let mut effects = technologies
        .iter()
        .map(|t| {
            let at = *rows
                .get(t.effect as usize)
                .ok_or_else(|| crate::invalid("DE survey", 0, "effect index out of range"))?;
            Ok((t.effect, effect(bytes, at)?.0))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    for (id, &at) in rows.iter().enumerate() {
        let e = effect(bytes, at)?.0;
        if e.name.to_ascii_lowercase().contains("scout")
            || e.name.to_ascii_lowercase().contains("palisade")
            || e.name.to_ascii_lowercase().contains("gate hp")
        {
            effects.push((id as i16, e));
        }
    }
    let projectiles = military::projectiles(bytes, &soldiers, &buildings)?;
    Ok(Survey {
        version: "VER 8.9".into(),
        source_hash: String::new(),
        inflated_bytes: bytes.len(),
        buildings,
        soldiers,
        workers: economy::workers(bytes)?,
        projectiles,
        animations: graphics::heroes(bytes)?,
        technologies,
        effects,
    })
}

pub fn inflate(compressed: &[u8]) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    const LIMIT: u64 = 128 * 1024 * 1024;
    let mut bytes = Vec::new();
    flate2::read::DeflateDecoder::new(compressed)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(crate::invalid(
            "DE survey",
            0,
            "inflated data exceeds 128 MiB",
        ));
    }
    Ok(bytes)
}

pub fn decode(compressed: &[u8]) -> Result<Survey, Error> {
    let bytes = inflate(compressed)?;
    let mut report = survey(&bytes)?;
    report.source_hash = blake3::hash(compressed).to_hex().to_string();
    Ok(report)
}

#[cfg(test)]
#[path = "tests/de_survey.rs"]
mod tests;
