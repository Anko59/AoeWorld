//! Resolve bounded, local DAT building recipes into the shared runtime atlas.
use super::manifest::{BuildingComposition, BuildingPart};
use super::*;

#[inline(never)]
pub(super) fn select(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<(), JsValue> {
    if manifest.buildings.len() > aoe_assets::catalog::MAX_COMPOSITIONS {
        return Err(error("too many building recipes"));
    }
    for (slot, building) in manifest.buildings.iter().enumerate() {
        if manifest.buildings[..slot]
            .iter()
            .any(|b| b.unit_id == building.unit_id)
            || building.parts.iter().filter(|p| p.primary).count() > 1
            || building.parts.len() > 256
            || building.construction.len() > 256
            || building.damage.len() > 7
            || building
                .footprint
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 128.0)
        {
            return Err(error("invalid building recipe"));
        }
        if material(building) == Some(17) {
            let slp = building
                .foundation_slp
                .ok_or_else(|| error("missing foundation SLP"))?;
            let [columns, rows] = building.foundation_size;
            let count = u32::from(columns) * u32::from(rows);
            if columns == 0 || rows == 0 || count as usize > MAX_SELECTED_FRAMES {
                return Err(error("invalid foundation topology"));
            }
            let source = format!("terrain.drs:[32, 112, 108, 115]:{slp}");
            let frames = source_frames(manifest, &source, count)
                .ok_or_else(|| error("missing DAT foundation terrain"))?;
            for index in frames {
                if !selected.contains(&index) {
                    if selected.len() == MAX_SELECTED_FRAMES {
                        return Err(error("too many foundation frames"));
                    }
                    selected.push(index);
                    domains.push(AtlasDomain::Terrain);
                }
            }
        }
        // Resolve each recipe slice through one loop, including state and damage
        // layers. Keeping the part loop outside iterator adapters avoids cloning
        // its validation and source-selection body for each recipe section.
        for group in [&building.parts, &building.construction]
            .into_iter()
            .chain(building.states.iter().map(|(_, parts)| parts))
            .chain(building.damage.iter().map(|d| &d.parts))
        {
            for part in group {
                if part.frames == 0
                    || part.angles == 0
                    || u32::from(part.frames) * u32::from(part.angles) > 16384
                    || !part.frame_rate.is_finite()
                    || part.frame_rate < 0.0
                    || !part.replay_delay.is_finite()
                    || part.replay_delay < 0.0
                {
                    return Err(error("invalid building animation"));
                }
                if part
                    .misplacement
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 128.0)
                    || part.offset.iter().any(|v| v.unsigned_abs() > 32768)
                {
                    return Err(error("invalid building part offset"));
                }
                let Some(record) = part.record else {
                    continue;
                };
                let source = &manifest.frames[record].source;
                let declared = u32::from(part.frames) * u32::from(part.angles);
                let native = manifest.frames[record].native.is_some();
                let count = if native {
                    manifest
                        .frames
                        .iter()
                        .filter(|f| f.source == *source)
                        .count() as u32
                } else {
                    declared
                };
                if count == 0
                    || count > 16384
                    || (count < declared && !(part.frame_rate == 0.0 && part.frames == 1))
                {
                    return Err(error("Incomplete native animation rows"));
                }
                let Some(frames) = source_frames(manifest, source, count) else {
                    return Err(error(format!("corrupt building art ({source})")));
                };
                let step = 1;
                for index in frames.into_iter().step_by(step) {
                    if !selected.contains(&index) {
                        if selected.len() == MAX_SELECTED_FRAMES {
                            return Err(error("too many selected building frames"));
                        }
                        selected.push(index);
                        domains.push(
                            if (605..=610).contains(&building.unit_id)
                                || aoe_assets::catalog::ENVIRONMENT_UNIT_IDS
                                    .contains(&building.unit_id)
                            {
                                AtlasDomain::Shared
                            } else {
                                AtlasDomain::Objects
                            },
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[inline(never)]
fn parts(
    unit_id: i32,
    parts: &[BuildingPart],
    manifest: &Manifest,
    selected: &[usize],
    records: &[GameFrame],
) -> Vec<aoe_rendering::BuildingPart> {
    let scale = if (605..=610).contains(&unit_id) {
        0.5
    } else {
        1.0
    };
    let scaled = |mut f: GameFrame| {
        f.size = f.size.map(|v| v * scale);
        f.anchor = f.anchor.map(|v| v * scale);
        f
    };
    let mut out = Vec::new();
    for part in parts {
        let Some(record) = part.record else {
            continue;
        };
        let source = &manifest.frames[record].source;
        let Some(slot) = selected.iter().position(|&i| i == record) else {
            // An absent trial resource is explicit: no partial assemblies and
            // no unrelated sprite substituted for the missing roof or camp.
            return Vec::new();
        };
        // Genie uses NE/SE tile axes; screen Y points down. The client's
        // world axes differ, so project these native coordinates directly.
        let [ne, se] = part.misplacement;
        let step = 1;
        let available = manifest
            .frames
            .iter()
            .filter(|f| f.source == *source)
            .count();
        // DAT static variant slots can include an absent placeholder. Animated
        // direction rows were validated above; terminal source cells stay selected.
        let angles = part
            .angles
            .min((available / usize::from(part.frames)) as u16);
        out.push(aoe_rendering::BuildingPart {
            frame: scaled(records[slot]),
            angles,
            animation: if u32::from(part.frames) * u32::from(angles) > 1 {
                (0..u32::from(part.frames) * u32::from(angles))
                    .step_by(step)
                    .filter_map(|number| {
                        selected
                            .iter()
                            .position(|&i| {
                                manifest.frames[i].source == *source
                                    && manifest.frames[i].frame == number
                            })
                            .map(|slot| scaled(records[slot]))
                    })
                    .collect()
            } else {
                Vec::new()
            },
            frame_rate: part.frame_rate,
            replay_delay: part.replay_delay,
            offset: [
                (part.offset[0] as f32 + (ne + se) * 48.0) * scale,
                (part.offset[1] as f32 + (se - ne) * 24.0) * scale,
            ],
            depth: se - ne,
        });
    }
    out
}

fn material(building: &BuildingComposition) -> Option<u8> {
    let slp = building.foundation_slp?;
    for (slot, role) in TERRAIN_ROLES.iter().enumerate() {
        if REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_TERRAIN_SOURCES.iter())
            .any(|s| s.role == *role && s.id as i32 == slp)
        {
            return Some([0, 1, 2, 3, 4, 5, 6, 13, 14, 15, 16, 7][slot]);
        }
    }
    Some(17)
}

pub(super) fn foundation(
    manifest: &Manifest,
    selected: &[usize],
    records: &[GameFrame],
) -> Result<(Vec<GameFrame>, Option<TerrainTopology>), JsValue> {
    let mut result = (Vec::new(), None);
    let mut slp = None;
    for building in manifest
        .buildings
        .iter()
        .filter(|b| material(b) == Some(17))
    {
        let id = building.foundation_slp;
        if slp.is_some() && slp != id {
            return Err(error("multiple additional foundation sheets"));
        }
        if slp == id {
            continue;
        }
        slp = id;
        let source = format!("terrain.drs:[32, 112, 108, 115]:{}", id.unwrap_or_default());
        let [columns, rows] = building.foundation_size;
        for number in 0..u32::from(columns) * u32::from(rows) {
            if let Some(slot) = selected.iter().position(|&i| {
                manifest.frames[i].source == source && manifest.frames[i].frame == number
            }) {
                result.0.push(records[slot]);
            }
        }
        result.1 = Some(TerrainTopology::PeriodicXMajorReversedY { columns, rows });
    }
    Ok(result)
}

pub(super) fn resolve(
    manifest: &Manifest,
    selected: &[usize],
    records: &[GameFrame],
) -> Vec<aoe_rendering::BuildingArt> {
    manifest
        .buildings
        .iter()
        .map(|b| aoe_rendering::BuildingArt {
            unit_id: b.unit_id,
            footprint: b.footprint,
            foundation_material: if b.unit_id == 50 && !manifest.terrains.is_empty() {
                Some(17)
            } else {
                material(b)
            },
            parts: parts(b.unit_id, &b.parts, manifest, selected, records),
            construction: parts(b.unit_id, &b.construction, manifest, selected, records),
            states: b
                .states
                .iter()
                .map(|(role, p)| (*role, parts(b.unit_id, p, manifest, selected, records)))
                .collect(),
            damage: b
                .damage
                .iter()
                .map(|d| aoe_rendering::BuildingDamage {
                    damage_percent: d.damage_percent as u8,
                    apply_mode: d.apply_mode,
                    parts: parts(-1, &d.parts, manifest, selected, records),
                })
                .collect(),
        })
        .collect()
}
