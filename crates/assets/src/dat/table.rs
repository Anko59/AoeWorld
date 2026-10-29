//! Name to graphic to SLP table built from a decoded [`DatFile`], with
//! cross-checks against sprite ids reviewed by eye. Local-only output.
use super::{DatFile, Graphic, Unit};
use crate::{Error, drs};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

/// Display name and Gaia unit id of the Dark Age units the client renders.
pub const DARK_AGE: &[(&str, i32)] = &[
    ("Villager (male)", 83),
    ("Villager (female)", 293),
    ("Militia", 74),
    ("Archer", 4),
    ("Scout Cavalry", 448),
    ("King (Vassal)", 434),
    ("Joan of Arc mounted (General)", 629),
    ("Town Center", 109),
    ("Town Center (Dark Age unit)", 618),
    ("House", 70),
    ("Mill", 68),
    ("Lumber Camp", 562),
    ("Mining Camp", 584),
    ("Farm", 50),
    ("Barracks", 12),
    ("Palisade Wall", 72),
    ("Outpost", 598),
    ("Dock", 45),
    ("Fishing Ship", 13),
    ("Fish (Perch)", 53),
    ("Fish (Shore)", 69),
    ("Fish (Salmon)", 456),
    ("Fish (Tuna)", 457),
    ("Fish (Snapper)", 458),
    ("Fish (Dorado)", 455),
    ("Fish (Bass)", 459),
    ("Deer", 65),
    ("Boar", 48),
    ("Sheep", 594),
    ("Wolf", 126),
    ("Forage bush", 59),
    ("Gold mine", 66),
    ("Stone mine", 102),
    ("Oak tree", 349),
    ("Pine tree", 350),
    ("Palm tree", 351),
    ("Tree stump", 415),
    ("Gather point flag", 720),
];

#[path = "feudal.rs"]
mod feudal;
pub use feudal::{CASTLE, FEUDAL, FEUDAL_MILITARY};
/// Villager variants share an internal-name stem; the suffix names the job.
const JOBS: &[(&str, &str)] = &[
    ("BAS", "base"),
    ("BLD", "builder"),
    ("FOR", "forager"),
    ("HUN", "hunter"),
    ("LUM", "lumberjack"),
    ("MIN", "stone miner"),
    ("GLD", "gold miner"),
    ("FAR", "farmer"),
    ("REP", "repairer"),
    ("FIS", "fisher"),
    ("SHE", "shepherd"),
];

const RESOURCES: [&str; 5] = ["food", "wood", "stone", "gold", "population"];

#[derive(Clone, Debug, Serialize)]
pub struct GraphicRef {
    pub role: String,
    pub graphic: i32,
    pub name: String,
    pub slp: i32,
    pub frames: u16,
    pub angles: u16,
    pub frame_rate: f32,
    pub in_pack: Option<bool>,
    pub layer: u8,
    pub deltas: Vec<super::GraphicDelta>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub label: String,
    pub unit: i32,
    pub internal_name: Option<String>,
    pub kind: Option<u8>,
    pub hit_points: Option<i16>,
    pub line_of_sight: Option<f32>,
    pub train_time: Option<i16>,
    pub cost: Vec<String>,
    pub gameplay: Option<Unit>,
    pub attacks: Vec<(i16, i16)>,
    pub armours: Vec<(i16, i16)>,
    pub graphics: Vec<GraphicRef>,
    /// Sibling graphics of a building sharing its name stem (age and part variants).
    pub family: Vec<GraphicRef>,
    /// `complete`, `partial` or `absent` against the supplied graphics.drs.
    pub pack: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub label: String,
    pub slp: i32,
    pub expected: String,
    pub status: String,
    pub dat_says: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Table {
    /// `complete`, or `INCOMPLETE: <stop reason>` when a decode stage stopped.
    pub status: String,
    pub version: String,
    pub report: super::Report,
    pub rows: Vec<Row>,
    pub checks: Vec<Check>,
    pub buildings: Vec<super::BuildingComposition>,
    pub building_records: Vec<(i32, super::Building)>,
    pub terrains: Vec<super::Terrain>,
    pub technologies: Vec<super::Technology>,
}

/// SLP resource ids of a classic DRS archive (graphics.drs).
pub fn slp_ids(drs_bytes: &[u8]) -> Result<BTreeSet<i32>, Error> {
    Ok(drs::parse(drs_bytes)?
        .iter()
        .filter(|entry| entry.kind == *b" pls")
        .filter_map(|entry| i32::try_from(entry.id).ok())
        .collect())
}

fn gref(file: &DatFile, role: &str, id: i32, pack: Option<&BTreeSet<i32>>) -> Option<GraphicRef> {
    let graphic = file.graphic(id).filter(|g| g.slp_id > 0)?;
    Some(from_graphic(role, graphic, pack))
}

fn from_graphic(role: &str, graphic: &Graphic, pack: Option<&BTreeSet<i32>>) -> GraphicRef {
    GraphicRef {
        role: role.to_owned(),
        graphic: graphic.id,
        name: graphic.name.clone(),
        slp: graphic.slp_id,
        frames: graphic.frame_count,
        angles: graphic.angle_count,
        frame_rate: graphic.frame_rate,
        in_pack: pack.map(|set| set.contains(&graphic.slp_id)),
        layer: graphic.layer,
        deltas: graphic.deltas.clone(),
    }
}

/// Every graphic the unit uses, tagged by role (tasks as `task<n>.<slot>`).
fn unit_graphics(file: &DatFile, unit: &Unit, pack: Option<&BTreeSet<i32>>) -> Vec<GraphicRef> {
    let mut wanted: Vec<(String, i32)> = vec![
        ("stand".into(), unit.standing[0].into()),
        ("dying".into(), unit.dying.into()),
    ];
    wanted.extend(unit.walking.map(|w| ("walk".into(), w[0].into())));
    wanted.extend(unit.attack_graphic.map(|a| ("attack".into(), a.into())));
    wanted.extend(
        file.unit(unit.dead_unit.into())
            .map(|dead| ("decay".into(), i32::from(dead.standing[0]))),
    );
    wanted.extend(
        unit.construction_graphic
            .map(|c| ("construction".into(), c.into())),
    );
    const SLOTS: [&str; 4] = ["moving", "proceeding", "working", "carrying"];
    for task in &unit.tasks {
        for (slot, graphic) in task.graphics.iter().enumerate() {
            wanted.push((
                format!("task{}.{}", task.id, SLOTS[slot]),
                (*graphic).into(),
            ));
        }
    }
    wanted
        .into_iter()
        .filter_map(|(role, id)| gref(file, &role, id, pack))
        .collect()
}

fn family(file: &DatFile, unit: &Unit, pack: Option<&BTreeSet<i32>>) -> Vec<GraphicRef> {
    let Some(stand) = file.graphic(unit.standing[0].into()) else {
        return Vec::new();
    };
    let Some(stem) = stand.name.get(..stand.name.len().saturating_sub(2)) else {
        return Vec::new();
    };
    let tree = matches!(unit.id, 349..=351);
    if (!tree && unit.kind != 80) || stem.len() < 4 {
        return Vec::new();
    }
    file.graphics
        .iter()
        .filter(|g| g.slp_id > 0 && g.name.starts_with(stem) && g.name.len() == stand.name.len())
        .map(|g| from_graphic("family", g, pack))
        .collect()
}

/// Each unit's graphics, computed once and shared by rows and checks.
type Cache = HashMap<i32, Vec<GraphicRef>>;

fn row(file: &DatFile, cache: &Cache, label: &str, id: i32, pack: Option<&BTreeSet<i32>>) -> Row {
    let Some(unit) = file.unit(id) else {
        return Row {
            label: label.to_owned(),
            unit: id,
            internal_name: None,
            kind: None,
            hit_points: None,
            line_of_sight: None,
            train_time: None,
            cost: Vec::new(),
            gameplay: None,
            attacks: Vec::new(),
            armours: Vec::new(),
            graphics: Vec::new(),
            family: Vec::new(),
            pack: "absent".into(),
        };
    };
    let graphics = cache.get(&id).cloned().unwrap_or_default();
    let needed: Vec<&GraphicRef> = graphics.iter().collect();
    let present = needed.iter().filter(|g| g.in_pack == Some(true)).count();
    let pack_state = match pack {
        None => "unchecked",
        Some(_) if present == 0 => "absent",
        Some(_) if needed.iter().all(|g| g.in_pack == Some(true)) => "complete",
        Some(_) => "partial",
    };
    Row {
        label: label.to_owned(),
        gameplay: Some(unit.clone()),
        unit: id,
        internal_name: Some(unit.name.clone()),
        kind: Some(unit.kind),
        hit_points: Some(unit.hit_points),
        line_of_sight: Some(unit.line_of_sight),
        train_time: unit.train_time.filter(|t| *t > 0),
        cost: unit
            .costs
            .iter()
            .filter(|c| c.enabled != 0 && c.amount > 0)
            .map(|c| {
                let name = usize::try_from(c.resource)
                    .ok()
                    .and_then(|i| RESOURCES.get(i));
                format!("{} {}", c.amount, name.copied().unwrap_or("?"))
            })
            .collect(),
        attacks: unit.attacks.clone(),
        armours: unit.armours.clone(),
        family: family(file, unit, pack),
        graphics,
        pack: pack_state.into(),
    }
}

/// Where a SLP id is referenced: `unit <id> <name> <role>`.
fn references(file: &DatFile, cache: &Cache, slp: i32) -> Vec<String> {
    let mut out = Vec::new();
    for unit in &file.units {
        for graphic in cache.get(&unit.id).into_iter().flatten() {
            if graphic.slp == slp {
                let role = if graphic.role.starts_with("task") {
                    "task"
                } else {
                    graphic.role.as_str()
                };
                out.push(format!(
                    "unit {} {} {} ({})",
                    unit.id, unit.name, role, graphic.name
                ));
            }
        }
    }
    out
}

enum Expect {
    Role(i32, &'static str),
    Family(i32),
}

/// Catalog sprite ids with their DAT unit and role; the DAT is authoritative.
const CHECKS: &[(&str, i32, Expect)] = &[
    ("Town Center (Dark Age main)", 891, Expect::Family(109)),
    ("Villager idle", 1479, Expect::Role(83, "stand")),
    ("Villager walk", 1484, Expect::Role(83, "walk")),
    ("Scout idle", 3004, Expect::Role(448, "stand")),
    ("Scout walk", 3008, Expect::Role(448, "walk")),
    ("Wood chopping", 1535, Expect::Role(123, "task")),
    ("Wood carrying", 1536, Expect::Role(123, "task")),
    ("Berry foraging", 2589, Expect::Role(120, "task")),
    ("Food carrying", 2592, Expect::Role(120, "task")),
    ("Farm tilling", 1512, Expect::Role(259, "task")),
    ("Gold mining", 1560, Expect::Role(579, "task")),
    ("Gold carrying", 2117, Expect::Role(579, "task")),
    ("Stone carrying", 1552, Expect::Role(124, "task")),
    ("Stone mining", 1560, Expect::Role(124, "task")),
];

fn check(file: &DatFile, cache: &Cache, label: &str, slp: i32, expect: &Expect) -> Check {
    let refs = references(file, cache, slp);
    let (expected, ok, family_ok) = match expect {
        Expect::Role(unit, role) => {
            let prefix = format!("unit {unit} ");
            let hit = refs
                .iter()
                .any(|r| r.starts_with(&prefix) && r.contains(&format!(" {role} (")));
            (format!("unit {unit} {role}"), hit, false)
        }
        Expect::Family(unit) => {
            let hit = file
                .unit(*unit)
                .is_some_and(|u| family(file, u, None).iter().any(|g| g.slp == slp));
            (format!("unit {unit} graphic family"), false, hit)
        }
    };
    let status = if ok {
        "match"
    } else if family_ok {
        "family"
    } else {
        "mismatch"
    };
    let mut dat_says = refs;
    if dat_says.is_empty() {
        dat_says = file
            .graphics
            .iter()
            .filter(|g| g.slp_id == slp)
            .map(|g| format!("graphic {} {} (no unit uses it directly)", g.id, g.name))
            .collect();
    }
    Check {
        label: label.to_owned(),
        slp,
        expected,
        status: status.into(),
        dat_says,
    }
}

pub fn build(file: &DatFile, pack: Option<&BTreeSet<i32>>) -> Table {
    let cache: Cache = file
        .units
        .iter()
        .map(|unit| (unit.id, unit_graphics(file, unit, pack)))
        .collect();
    let mut rows: Vec<Row> = DARK_AGE
        .iter()
        .chain(FEUDAL_MILITARY.iter().chain(FEUDAL).chain(CASTLE))
        .map(|(label, id)| row(file, &cache, label, *id, pack))
        .collect();
    for unit in file.units.iter().filter(|u| u.kind == 70) {
        let stem = unit.name.get(..2).unwrap_or("");
        let job = JOBS
            .iter()
            .find(|(suffix, _)| unit.name.get(2..) == Some(*suffix));
        if (stem == "VM" || stem == "VF")
            && let Some((_, job)) = job
            && rows.iter().all(|r| r.unit != unit.id)
        {
            let sex = if stem == "VM" { "male" } else { "female" };
            rows.push(row(
                file,
                &cache,
                &format!("Villager {job} ({sex})"),
                unit.id,
                pack,
            ));
        }
    }
    Table {
        status: file.report.stopped.as_ref().map_or_else(
            || "complete".to_owned(),
            |reason| format!("INCOMPLETE: {reason}"),
        ),
        version: file.version.clone(),
        report: file.report.clone(),
        buildings: super::table_details::compositions(file, pack),
        building_records: file
            .units
            .iter()
            .filter_map(|u| u.building.clone().map(|b| (u.id, b)))
            .collect(),
        terrains: file.terrains.clone(),
        technologies: file.technologies.clone(),
        rows,
        checks: CHECKS
            .iter()
            .map(|(l, s, e)| check(file, &cache, l, *s, e))
            .collect(),
    }
}

pub fn markdown(table: &Table) -> String {
    let mut out = format!("# empires2.dat table ({}), LOCAL ONLY\n\n", table.version);
    if let Some(reason) = &table.report.stopped {
        let _ = writeln!(out, "**INCOMPLETE: {reason}**\n");
    }
    for row in &table.rows {
        let _ = writeln!(
            out,
            "## {} (unit {}, {}) pack: {}\n",
            row.label,
            row.unit,
            row.internal_name
                .as_deref()
                .unwrap_or("missing from this dat"),
            row.pack
        );
        for g in row.graphics.iter().chain(row.family.iter()) {
            let _ = writeln!(
                out,
                "- {} graphic {} {} SLP {} frames {} angles {} in pack {:?}",
                g.role, g.graphic, g.name, g.slp, g.frames, g.angles, g.in_pack
            );
        }
        out.push('\n');
    }
    super::table_details::markdown(table, &mut out);
    out.push_str("## Cross-checks\n\n");
    for c in &table.checks {
        let _ = writeln!(
            out,
            "- {} {}: {} (expected {}); dat: {}",
            c.status,
            c.label,
            c.slp,
            c.expected,
            c.dat_says.join("; ")
        );
    }
    out
}

pub use super::table_details::summary;
