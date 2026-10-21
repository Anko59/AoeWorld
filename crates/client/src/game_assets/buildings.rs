//! Resolve bounded, local DAT building recipes into the shared runtime atlas.
use super::manifest::{BuildingComposition, BuildingPart};
use super::*;

pub(super) fn select(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<(), JsValue> {
    if manifest.buildings.len() > 32 {
        return Err(error("too many building recipes"));
    }
    for (slot, building) in manifest.buildings.iter().enumerate() {
        if manifest.buildings[..slot]
            .iter()
            .any(|b| b.unit_id == building.unit_id)
            || building.parts.iter().filter(|p| p.primary).count() > 1
            || building.parts.len() > 256
            || building.construction.len() > 256
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
        for part in building.parts.iter().chain(&building.construction) {
            if part.frames == 0
                || part.angles == 0
                || u32::from(part.frames) * u32::from(part.angles) > 256
                || !part.frame_rate.is_finite()
                || part.frame_rate < 0.0
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
            let count = u32::from(part.frames) * u32::from(part.angles);
            let Some(frames) = source_frames(manifest, source, count) else {
                return Err(error(format!("corrupt building art ({source})")));
            };
            // A standing palisade displays one damage frame per native angle.
            // Its connectivity selects the angle; these frames do not animate.
            let step = aoe_assets::catalog::runtime::building_frame_step(
                building.unit_id,
                part.frames,
                part.angles,
                part.frame_rate,
            );
            for index in frames.into_iter().step_by(step) {
                if !selected.contains(&index) {
                    if selected.len() == MAX_SELECTED_FRAMES {
                        return Err(error("too many selected building frames"));
                    }
                    selected.push(index);
                    domains.push(AtlasDomain::Objects);
                }
            }
        }
    }
    Ok(())
}

fn parts(
    unit_id: i32,
    parts: &[BuildingPart],
    manifest: &Manifest,
    selected: &[usize],
    records: &[GameFrame],
) -> Vec<aoe_rendering::BuildingPart> {
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
        let step = aoe_assets::catalog::runtime::building_frame_step(
            unit_id,
            part.frames,
            part.angles,
            part.frame_rate,
        );
        out.push(aoe_rendering::BuildingPart {
            frame: records[slot],
            angles: part.angles,
            animation: if u32::from(part.frames) * u32::from(part.angles) > 1 {
                (0..u32::from(part.frames) * u32::from(part.angles))
                    .step_by(step)
                    .filter_map(|number| {
                        selected
                            .iter()
                            .position(|&i| {
                                manifest.frames[i].source == *source
                                    && manifest.frames[i].frame == number
                            })
                            .map(|slot| records[slot])
                    })
                    .collect()
            } else {
                Vec::new()
            },
            frame_rate: part.frame_rate * if unit_id == 72 { 1.0 } else { step as f32 },
            offset: [
                part.offset[0] as f32 + (ne + se) * 48.0,
                part.offset[1] as f32 + (se - ne) * 24.0,
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
            foundation_material: material(b),
            parts: parts(b.unit_id, &b.parts, manifest, selected, records),
            construction: parts(b.unit_id, &b.construction, manifest, selected, records),
        })
        .collect()
}
