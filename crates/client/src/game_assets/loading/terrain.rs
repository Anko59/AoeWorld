//! Bind semantic ground roles to the current DAT terrain directory.
use super::*;
use aoe_assets::pack::TerrainSheet;
use std::ops::Range;

/// DAT terrain per renderer slot; the last, 24 (g_rd1), is the dirt road.
const IDS: [u16; 17] = [
    12, 100, 6, 14, 40, 1, 10, 11, 9, 0, 2, 124, 23, 22, 26, 4, 24,
];
type SelectedSheet = (Range<usize>, TerrainTopology);
pub(super) type FarmSheets = [Option<SelectedSheet>; 2];

pub(super) fn selection(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
    ranges: &mut RoleRanges,
    topologies: &mut [Option<TerrainTopology>; 12],
    depths: &mut [Option<TerrainTopology>; 5],
) -> Result<FarmSheets, JsValue> {
    if manifest.terrains.is_empty() {
        return Ok([None, None]);
    }
    let mut select = |sheet: &TerrainSheet| -> Result<SelectedSheet, JsValue> {
        let topology = TerrainTopology::Dds {
            size: sheet.size,
            period: sheet.period,
            level: sheet.size[0].max(sheet.size[1]).ilog2().saturating_sub(8) as u8,
        };
        let count = topology
            .dds_frames()
            .ok_or("Invalid native terrain topology")?;
        let frames = source_frames(manifest, &sheet.source, count as u32)
            .ok_or("Incomplete native terrain mip chain")?;
        if frames.iter().any(|&i| manifest.frames[i].native.is_none()) {
            return Err(error("Native terrain must use chunks"));
        }
        if selected.len() + frames.len() > MAX_SELECTED_FRAMES {
            return Err(error("Too many terrain frames"));
        }
        let start = selected.len();
        domains.extend(std::iter::repeat_n(AtlasDomain::Terrain, frames.len()));
        selected.extend(frames);
        Ok((start..selected.len(), topology))
    };
    for (slot, id) in IDS.into_iter().enumerate() {
        let sheet = manifest
            .terrains
            .iter()
            .find(|t| t.id == id)
            .ok_or("Missing DAT terrain binding")?;
        let (range, topology) = select(sheet)?;
        let role = if slot < 12 {
            TERRAIN_ROLES[slot]
        } else {
            WATER_DEPTH_ROLES[slot - 12]
        };
        ranges[role as usize] = Some(range);
        if slot < 12 {
            topologies[slot] = Some(topology);
        } else {
            depths[slot - 12] = Some(topology);
        }
    }
    // DAT 7 is the planted surface (g_fm1). DAT 29 is Farm, 0% (g_fc1).
    let mut farms = [None, None];
    for (id, target) in [7, 29].into_iter().zip(&mut farms) {
        *target = manifest
            .terrains
            .iter()
            .find(|t| t.id == id)
            .map(&mut select)
            .transpose()?;
    }
    Ok(farms)
}
