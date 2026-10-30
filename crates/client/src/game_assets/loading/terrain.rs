//! Bind semantic ground roles to the current DAT terrain directory.
use super::*;
const IDS: [u16; 16] = [12, 100, 6, 14, 40, 1, 10, 11, 9, 0, 2, 124, 23, 22, 26, 4];
pub(super) fn selection(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
    ranges: &mut RoleRanges,
    topologies: &mut [Option<TerrainTopology>; 12],
    depths: &mut [Option<TerrainTopology>; 4],
) -> Result<Option<(std::ops::Range<usize>, TerrainTopology)>, JsValue> {
    if manifest.terrains.is_empty() {
        return Ok(None);
    }
    for (slot, id) in IDS.into_iter().enumerate() {
        let sheet = manifest
            .terrains
            .iter()
            .find(|t| t.id == id)
            .ok_or("Missing DAT terrain binding")?;
        let role = if slot < 12 {
            TERRAIN_ROLES[slot]
        } else {
            WATER_DEPTH_ROLES[slot - 12]
        };
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
        selected.extend(frames);
        domains.extend(std::iter::repeat_n(
            AtlasDomain::Terrain,
            selected.len() - start,
        ));
        ranges[role as usize] = Some(start..selected.len());
        if slot < 12 {
            topologies[slot] = Some(topology);
        } else {
            depths[slot - 12] = Some(topology);
        }
    }
    if let Some(sheet) = manifest.terrains.iter().find(|t| t.id == 7) {
        let topology = TerrainTopology::Dds {
            size: sheet.size,
            period: sheet.period,
            level: sheet.size[0].max(sheet.size[1]).ilog2().saturating_sub(8) as u8,
        };
        let count = topology.dds_frames().ok_or("Invalid farm terrain")?;
        let frames = source_frames(manifest, &sheet.source, count as u32)
            .ok_or("Incomplete farm terrain")?;
        if selected.len() + frames.len() > MAX_SELECTED_FRAMES {
            return Err(error("Too many farm cells"));
        }
        let start = selected.len();
        domains.extend(std::iter::repeat_n(AtlasDomain::Terrain, frames.len()));
        selected.extend(frames);
        return Ok(Some((start..selected.len(), topology)));
    }
    Ok(None)
}
