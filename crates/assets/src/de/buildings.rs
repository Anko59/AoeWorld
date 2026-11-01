//! One DE DAT pass for every gameplay building, its annexes and graphic states.
use super::*;
use crate::{
    dat::{self, de_survey, de_units},
    invalid,
};
use de_survey::graphics::{Graphic, Part};
use std::collections::{BTreeMap, BTreeSet};

pub struct Catalog {
    pub recipes: Vec<dat::BuildingComposition>,
    pub sources: BTreeSet<String>,
}
fn family(head: &str, part: &str) -> Option<String> {
    let (prefix, _) = head.split_once("_age")?;
    let (_, tail) = part.split_once("_age")?;
    let age = head.split_once("_age")?.1.chars().next()?;
    Some(format!("{prefix}_age{age}{}", tail.get(1..)?))
}
struct Resolver<'a> {
    graphics: &'a BTreeMap<i16, Graphic>,
    units: &'a BTreeMap<i16, de_units::Unit>,
    directory: PathBuf,
    disabled: BTreeSet<i16>,
    sources: BTreeSet<String>,
}
impl Resolver<'_> {
    fn parts(&mut self, id: i16, position: [f32; 2]) -> Result<Vec<dat::BuildingPart>, Error> {
        if id < 0 {
            return Ok(Vec::new());
        }
        let graphic = self
            .graphics
            .get(&id)
            .ok_or_else(|| invalid("DE building", 0, format!("missing graphic {id}")))?;
        let mut out = Vec::new();
        for Part {
            animation: a,
            graphic,
            x,
            y,
            layer,
        } in &graphic.parts
        {
            // A filename is authoritative even when the obsolete classic SLP ID is -1.
            let file = if self.directory.join(format!("{}.sld", a.file)).is_file() {
                a.file.clone()
            } else {
                self.graphics
                    .values()
                    .flat_map(|g| &g.parts)
                    .find(|p| {
                        a.sprite >= 0
                            && p.animation.sprite == a.sprite
                            && self
                                .directory
                                .join(format!("{}.sld", p.animation.file))
                                .is_file()
                    })
                    .map(|p| p.animation.file.clone())
                    .or_else(|| (a.sprite < 0).then(|| a.file.clone()))
                    .ok_or_else(|| {
                        invalid(
                            "DE building",
                            0,
                            format!("missing sprite {} ({})", a.sprite, a.file),
                        )
                    })?
            };
            if file.contains('/') || file.contains('\\') {
                return Err(invalid("DE building", 0, "unsafe source"));
            }
            let source = format!("de/{file}.sld");
            let available = self.directory.join(format!("{file}.sld")).is_file();
            if available {
                self.sources.insert(source.clone());
            }
            out.push(dat::BuildingPart {
                graphic_id: i32::from(*graphic),
                slp_id: a.sprite,
                source,
                available,
                frames: a.frames,
                angles: a.angles,
                primary: false,
                frame_rate: a.seconds,
                replay_delay: a.replay_delay,
                offset: [*x, *y],
                misplacement: position,
                layer: *layer,
            });
        }
        Ok(out)
    }
    fn annexes(
        &mut self,
        id: i16,
        head: Option<&str>,
        position: [f32; 2],
        seen: &mut BTreeSet<i16>,
        out: &mut Vec<dat::BuildingPart>,
    ) -> Result<(), Error> {
        if self.disabled.contains(&id) {
            return Ok(());
        }
        if seen.len() >= 16 || !seen.insert(id) || out.len() > 256 {
            return Err(invalid("DE building", 0, "annex tree bound"));
        }
        let unit = self
            .units
            .get(&id)
            .ok_or_else(|| invalid("DE building", 0, format!("missing annex {id}")))?;
        let mut graphic = unit.standing;
        if let Some(head) = head
            && let Some(g) = self.graphics.get(&graphic)
            && let Some(p) = g.parts.iter().find(|p| p.graphic == graphic)
            && let Some(file) = family(head, &p.animation.file)
            && let Some((&found, _)) = self.graphics.iter().find(|(_, g)| {
                g.parts
                    .iter()
                    .any(|p| p.animation.file == file && p.x == 0 && p.y == 0)
            })
        {
            graphic = found;
        }
        out.extend(self.parts(graphic, position)?);
        if let Some(b) = &unit.building {
            for a in &b.annexes {
                self.annexes(
                    a.unit,
                    head,
                    [position[0] + a.offset[0], position[1] + a.offset[1]],
                    seen,
                    out,
                )?;
            }
        }
        seen.remove(&id);
        Ok(())
    }
}
pub fn resolve(install: &Path) -> Result<Catalog, Error> {
    let bytes = read_bounded(&install.join("resources/_common/dat/empires2_x2_p1.dat"))?;
    let dat = de_survey::inflate(&bytes)?;
    let ids: Vec<_> = crate::catalog::GAMEPLAY_BUILDING_IDS
        .iter()
        .chain(&crate::catalog::ENVIRONMENT_UNIT_IDS)
        .map(|&id| id as i16)
        .collect();
    let units = de_units::find(&dat, &ids)?;
    let graphics = de_survey::graphics::indexed(&dat)?;
    let mut r = Resolver {
        graphics: &graphics,
        units: &units,
        directory: install.join(GRAPHICS),
        disabled: de_units::disabled(&dat)?,
        sources: BTreeSet::new(),
    };
    let mut recipes = Vec::new();
    for id in ids {
        let u = &units[&id];
        if u.building.is_none() {
            let parts = r.parts(u.standing, [0.0; 2])?;
            if crate::catalog::ENVIRONMENT_UNIT_IDS.contains(&i32::from(id))
                && (parts.is_empty() || parts.iter().any(|p| !p.available))
            {
                return Err(invalid(
                    "DE environment",
                    0,
                    "missing native resource graphic",
                ));
            }
            recipes.push(dat::BuildingComposition {
                unit_id: i32::from(id),
                footprint: u.footprint.map(|v| v.max(1.0)),
                foundation_terrain: -1,
                foundation_slp: None,
                foundation_size: [0; 2],
                parts,
                construction: Vec::new(),
                states: vec![
                    dat::BuildingState {
                        role: 0,
                        parts: r.parts(u.dying, [0.0; 2])?,
                    },
                    dat::BuildingState {
                        role: 1,
                        parts: r.parts(units.get(&u.dead).map_or(-1, |u| u.standing), [0.0; 2])?,
                    },
                ],
                damage: Vec::new(),
            });
            continue;
        }
        let b = u
            .building
            .as_ref()
            .ok_or_else(|| invalid("DE building", 0, "missing building tail"))?;
        let head = graphics
            .get(&u.standing)
            .and_then(|g| g.parts.iter().find(|p| p.graphic == u.standing))
            .map(|p| p.animation.file.as_str());
        let mut parts = Vec::new();
        r.annexes(id, head, [0.0; 2], &mut BTreeSet::new(), &mut parts)?;
        parts.sort_by(|a, b| {
            a.layer.cmp(&b.layer).then_with(|| {
                (a.misplacement[1] - a.misplacement[0])
                    .total_cmp(&(b.misplacement[1] - b.misplacement[0]))
            })
        });
        if let Some(p) = parts
            .iter_mut()
            .find(|p| p.graphic_id == i32::from(u.standing) && p.misplacement == [0.0; 2])
        {
            p.primary = true;
        }
        let construction = r.parts(b.construction, [0.0; 2])?;
        let mut states = Vec::new();
        // The unit dying/corpse links are the architecture-specific graphics;
        // building destruction/rubble fields may instead name generic debris.
        for (role, graphic) in [
            (0, u.dying),
            (1, units.get(&u.dead).map_or(b.rubble, |u| u.standing)),
            (2, b.snow),
            (3, b.researching),
            (4, b.research_complete),
            (6, b.destruction),
            (7, b.rubble),
        ] {
            states.push(dat::BuildingState {
                role,
                parts: r.parts(graphic, [0.0; 2])?,
            });
        }
        let mut open = parts.clone();
        let mut changed = false;
        for p in &mut open {
            let file = p
                .source
                .trim_start_matches("de/")
                .trim_end_matches(".sld")
                .replace("_closed", "_open");
            if file == p.source.trim_start_matches("de/").trim_end_matches(".sld") {
                continue;
            }
            if let Some((&graphic, _)) = graphics
                .iter()
                .find(|(_, g)| g.parts.iter().any(|p| p.animation.file == file))
                && let Some(mut replacement) = r.parts(graphic, p.misplacement)?.into_iter().next()
            {
                replacement.offset = p.offset;
                *p = replacement;
                changed = true;
            }
        }
        if changed {
            states.push(dat::BuildingState {
                role: 5,
                parts: open,
            });
        }
        let damage = u
            .damage
            .iter()
            .map(|d| {
                Ok(dat::BuildingDamage {
                    damage_percent: d.percent,
                    apply_mode: d.mode,
                    parts: r.parts(d.graphic, [0.0; 2])?,
                })
            })
            .collect::<Result<_, Error>>()?;
        recipes.push(dat::BuildingComposition {
            unit_id: i32::from(id),
            footprint: u.footprint,
            foundation_terrain: b.foundation,
            foundation_slp: None,
            foundation_size: [0; 2],
            parts,
            construction,
            states,
            damage,
        });
    }
    Ok(Catalog {
        recipes,
        sources: r.sources,
    })
}
