//! One DAT-driven unit pass. Bindings name semantic actions, never image files.
use crate::{
    Error,
    catalog::units::Pose,
    dat::{de_survey, de_units},
    invalid,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
mod bindings;
pub use bindings::BINDINGS;
mod scene;
#[derive(Clone, Copy)]
pub enum Action {
    Idle,
    Walk,
    Run,
    Attack,
    Death,
    Decay,
    Work,
    Carry,
}
pub struct Binding {
    pub pose: Pose,
    pub unit: i16,
    pub action: Action,
}
#[derive(Clone, Debug)]
pub struct Selected {
    pub pose: usize,
    pub sprite: u32,
    pub file: String,
    pub frames: u16,
    pub angles: u16,
    pub cells: u16,
    pub seconds: f32,
    pub replay: f32,
    pub terminal: bool,
    pub parts: Vec<de_survey::graphics::Part>,
}

pub fn resolve(install: &Path) -> Result<Vec<Selected>, Error> {
    let bytes = super::read_bounded(&install.join("resources/_common/dat/empires2_x2_p1.dat"))?;
    let dat = de_survey::inflate(&bytes)?;
    let ids: Vec<_> = bindings::BINDINGS
        .iter()
        .map(|b| b.unit)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let units = de_units::find(&dat, &ids)?;
    let graphics = de_survey::graphics::indexed(&dat)?;
    let directory = install.join(super::GRAPHICS);
    let mut selected = Vec::new();
    let mut inventories = BTreeMap::new();
    let mut files = BTreeMap::new();
    for g in graphics.values() {
        for p in &g.parts {
            let a = &p.animation;
            let high = a.file.strip_suffix("_x1").map(|s| format!("{s}_x2"));
            if let Some(file) = high
                .filter(|s| directory.join(format!("{s}.sld")).is_file())
                .or_else(|| {
                    directory
                        .join(format!("{}.sld", a.file))
                        .is_file()
                        .then(|| a.file.clone())
                })
            {
                files.entry(a.sprite).or_insert(file);
            }
        }
    }
    for binding in bindings::BINDINGS {
        let u = &units[&binding.unit];
        let idle = u.standing;
        let valid = |value: i16, fallback: i16| if value >= 0 { value } else { fallback };
        let graphic = match binding.action {
            Action::Idle => idle,
            Action::Walk => valid(u.walking, idle),
            Action::Run => valid(u.running, valid(u.walking, idle)),
            Action::Attack => valid(u.attack, valid(u.work, idle)),
            Action::Work => valid(u.work, valid(u.attack, idle)),
            Action::Carry => valid(u.carrying, valid(u.walking, idle)),
            Action::Death => valid(u.dying, idle),
            Action::Decay => units.get(&u.dead).map_or(valid(u.dying, idle), |corpse| {
                valid(corpse.standing, valid(u.dying, idle))
            }),
        };

        let g = graphics.get(&graphic).ok_or_else(|| {
            invalid(
                "DE unit",
                0,
                format!("unit {} {} missing graphic {graphic}", u.id, u.name),
            )
        })?;
        let mut parts = g.parts.clone();
        let mut raw_cells = 0;
        for part in &mut parts {
            let g = &mut part.animation;
            let high = g.file.strip_suffix("_x1").map(|stem| format!("{stem}_x2"));
            g.file = high
                .filter(|file| directory.join(format!("{file}.sld")).is_file())
                .or_else(|| {
                    directory
                        .join(format!("{}.sld", g.file))
                        .is_file()
                        .then(|| g.file.clone())
                })
                .or_else(|| files.get(&g.sprite).cloned())
                .ok_or_else(|| {
                    invalid(
                        "DE unit",
                        0,
                        format!("missing sprite {} ({})", g.sprite, g.file),
                    )
                })?;
            if g.file.contains('/') || g.file.contains('\\') {
                return Err(invalid("DE unit", 0, "unsafe graphic name"));
            }
            let cells = if let Some(&count) = inventories.get(&g.file) {
                count
            } else {
                let path = directory.join(format!("{}.sld", g.file));
                let bytes = super::read_bounded(&path)
                    .map_err(|e| invalid("DE unit", 0, format!("{}: {e}", path.display())))?;
                if bytes.get(..6) != Some(&[83, 76, 68, 88, 4, 0]) || bytes.len() < 16 {
                    return Err(invalid("DE unit", 0, "SLD header"));
                }
                let count = u16::from_le_bytes([bytes[6], bytes[7]]);
                inventories.insert(g.file.clone(), count);
                count
            };
            if u32::from(cells) < u32::from(g.frames) * u32::from(g.angles) {
                return Err(invalid("DE unit", 0, "incomplete native direction rows"));
            }
            raw_cells = cells;
        }
        let mut seen = BTreeSet::new();
        parts.retain(|p| seen.insert((p.animation.file.clone(), p.x, p.y)));
        let master = parts
            .iter()
            .filter(|p| p.animation.frames > 1)
            .min_by(|a, b| a.animation.seconds.total_cmp(&b.animation.seconds))
            .unwrap_or(&parts[0]);
        let (frames, angles, seconds, replay) = (
            master.animation.frames,
            parts
                .iter()
                .map(|p| p.animation.angles)
                .max()
                .ok_or_else(|| invalid("DE unit", 0, "empty graphic parts"))?,
            master.animation.seconds,
            master.animation.replay_delay,
        );
        for part in &parts {
            let a = &part.animation;
            if a.frames > 1
                && (a.frames != frames
                    || (a.seconds - seconds).abs() > 0.000001
                    || a.replay_delay != replay)
            {
                return Err(invalid(
                    "DE unit",
                    0,
                    format!("incommensurate delta timeline for graphic {graphic}"),
                ));
            }
            if a.angles != angles && a.angles != 1 {
                return Err(invalid("DE unit", 0, "incompatible delta directions"));
            }
        }
        let cells = if parts.len() == 1 {
            raw_cells
        } else {
            frames
                .checked_mul(angles)
                .ok_or_else(|| invalid("DE unit", 0, "composite cells"))?
        };
        let terminal = matches!(binding.action, Action::Decay) && graphic == u.dying;
        let file = format!("dat_{graphic}");
        selected.push(Selected {
            pose: binding.pose as usize,
            sprite: 200_000 + graphic as u32,
            file,
            frames,
            angles,
            cells,
            seconds,
            replay,
            terminal,
            parts,
        });
    }
    Ok(selected)
}

/// Regenerate the one current numeric catalog from the licensed DAT. No pixels
/// or source digests enter these checked-in metadata tables.
pub fn generate(install: &Path) -> Result<(), Error> {
    let selected = resolve(install)?;
    let mut unique = BTreeMap::new();
    for s in &selected {
        if let Some(old) = unique.insert(s.sprite, s)
            && (old.file != s.file
                || old.frames != s.frames
                || old.angles != s.angles
                || old.seconds != s.seconds
                || old.replay != s.replay)
        {
            return Err(invalid(
                "DE unit",
                0,
                "conflicting sprite animation metadata",
            ));
        }
    }
    let directory = Path::new("crates/assets/src/catalog/units/native/generated");
    fs::create_dir_all(directory)?;
    let entries: Vec<_> = unique.into_iter().collect();
    let mut graphics = String::from(
        "//! Generated by make assets-unit-table-de. Numeric DAT metadata only.\nuse super::Graphic;\n",
    );
    for (index, chunk) in entries.chunks(24).enumerate() {
        let mut text = String::from(
            "//! Generated numeric DAT animation descriptors.\nuse super::super::Graphic;\npub(super) const GRAPHICS:&[(u32,Graphic)]=&[\n",
        );
        for &(id, s) in chunk {
            text.push_str(&format!("({id},Graphic {{stem:{:?},cells:{},seconds:f32::from_bits({}),replay_delay:f32::from_bits({}),angles:{},stride:{}}}),\n",s.file,s.cells,s.seconds.to_bits(),s.replay.to_bits(),s.angles,s.frames));
        }
        text.push_str("];\n");
        fs::write(directory.join(format!("part{index}.rs")), text)?;
        graphics.push_str(&format!(
            "#[path=\"generated/part{index}.rs\"] mod part{index};\n"
        ));
    }
    graphics.push_str("pub(super) const fn get(id:u32)->Option<Graphic> {\n");
    for (index, chunk) in entries.chunks(24).enumerate() {
        graphics.push_str(&format!(
            "if id<={} {{return super::lookup(part{index}::GRAPHICS,id);}}\n",
            chunk
                .last()
                .ok_or_else(|| invalid("DE unit", 0, "empty generated chunk"))?
                .0
        ));
    }
    graphics.push_str("None\n}\n");
    let count = selected.iter().map(|s| s.pose).max().unwrap_or(0) + 1;
    let by_pose: BTreeMap<_, _> = selected.iter().map(|s| (s.pose, s)).collect();
    let mut poses = format!(
        "//! Generated by make assets-unit-table-de. Pose order is the semantic contract.\nuse super::*;\npub const SPRITES:[ActorSource;{count}]=[\n"
    );
    for pose in 0..count {
        if let Some(s) = by_pose.get(&pose) {
            poses.push_str(&format!(
                "sprite({}, {}, {}),\n",
                s.sprite,
                if s.terminal { 1 } else { s.frames },
                if s.terminal {
                    0
                } else {
                    (s.seconds * 1000.0).round() as u16
                }
            ));
        } else {
            let old = crate::catalog::units::SPRITES[pose];
            poses.push_str(&format!(
                "ActorSource {{id:{},stride:{},millis:{},phases:{}}},\n",
                old.id, old.stride, old.millis, old.phases
            ));
        }
    }
    poses.push_str("];\n");
    scene::generate(&selected, count)?;
    fs::write(
        "crates/assets/src/catalog/units/native/generated.rs",
        graphics,
    )?;
    fs::write("crates/assets/src/catalog/units/generated.rs", poses)?;
    let metadata:Vec<_>=selected.iter().map(|s|serde_json::json!({"pose":s.pose,"sprite":s.sprite,"file":s.file,"frames":s.frames,"angles":s.angles,"cells":s.cells,"seconds":s.seconds,"replay":s.replay})).collect();
    fs::write(
        "local-assets/unit-catalog.json",
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(())
}
