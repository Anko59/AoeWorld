//! Data-only building recipes. Annex depth uses the engine's isometric axes;
//! each unit's stand and delta list retain their source order within that depth.
use super::DatFile;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildingPart {
    pub graphic_id: i32,
    pub slp_id: i32,
    pub source: String,
    /// Set by the importer/table from the actual DRS inventory. The engine
    /// skips unavailable SLPs (the trial omits some referenced ground layers).
    pub available: bool,
    pub frames: u16,
    pub angles: u16,
    pub primary: bool,
    /// Seconds per frame (zero selects a static building variant).
    pub frame_rate: f32,
    pub offset: [i32; 2],
    pub misplacement: [f32; 2],
    pub layer: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildingComposition {
    pub unit_id: i32,
    pub footprint: [f32; 2],
    pub foundation_terrain: i16,
    pub foundation_slp: Option<i32>,
    pub foundation_size: [u16; 2],
    pub parts: Vec<BuildingPart>,
    pub construction: Vec<BuildingPart>,
}

impl DatFile {
    pub fn building_composition(&self, id: i32) -> Result<BuildingComposition, crate::Error> {
        let unit = self
            .unit(id)
            .ok_or_else(|| crate::invalid("composition", 0, "missing unit"))?;
        let building = unit
            .building
            .as_ref()
            .ok_or_else(|| crate::invalid("composition", 0, "not a building"))?;
        let mut units = vec![(id, [0.0; 2])];
        units.extend(
            building
                .annexes
                .iter()
                .filter(|a| a.unit_id >= 0)
                .map(|a| (i32::from(a.unit_id), a.misplacement)),
        );
        units.sort_by(|a, b| (a.1[1] - a.1[0]).total_cmp(&(b.1[1] - b.1[0])));
        let mut parts = Vec::new();
        for (id, position) in units {
            let unit = self
                .unit(id)
                .ok_or_else(|| crate::invalid("composition", 0, format!("missing annex {id}")))?;
            self.append_graphic(
                i32::from(unit.standing[0]),
                [0; 2],
                position,
                &mut Vec::new(),
                &mut parts,
            )?;
        }
        // Shadow layers precede all bodies. Stable sorting leaves every
        // same-layer stand/delta list and equal-depth annex in DAT order.
        for part in &mut parts {
            part.primary =
                part.graphic_id == i32::from(unit.standing[0]) && part.misplacement == [0.0; 2];
        }
        parts.sort_by_key(|p| p.layer);
        let mut construction = Vec::new();
        self.append_graphic(
            i32::from(building.construction_graphic),
            [0; 2],
            [0.0; 2],
            &mut Vec::new(),
            &mut construction,
        )?;
        construction.sort_by_key(|p| p.layer);
        Ok(BuildingComposition {
            unit_id: id,
            footprint: unit.footprint,
            foundation_terrain: building.foundation_terrain,
            foundation_slp: self.terrain_slp(building.foundation_terrain),
            foundation_size: self
                .terrain_texture(building.foundation_terrain)
                .map_or([0; 2], |t| t.dimensions),
            parts,
            construction,
        })
    }

    fn append_graphic(
        &self,
        id: i32,
        offset: [i32; 2],
        misplacement: [f32; 2],
        chain: &mut Vec<i32>,
        out: &mut Vec<BuildingPart>,
    ) -> Result<(), crate::Error> {
        if id < 0 {
            return Ok(());
        }
        if chain.contains(&id) || chain.len() >= 16 || out.len() >= 256 {
            return Err(crate::invalid(
                "composition",
                0,
                "cyclic or oversized graphic deltas",
            ));
        }
        let graphic = self
            .graphic(id)
            .ok_or_else(|| crate::invalid("composition", 0, format!("missing graphic {id}")))?;
        chain.push(id);
        if graphic.slp_id >= 0 {
            out.push(BuildingPart {
                graphic_id: id,
                slp_id: graphic.slp_id,
                source: format!("graphics.drs:[32, 112, 108, 115]:{}", graphic.slp_id),
                available: true,
                frames: graphic.frame_count.max(1),
                angles: graphic.angle_count.max(1),
                primary: false,
                frame_rate: graphic.frame_rate,
                offset,
                misplacement,
                layer: graphic.layer,
            });
        }
        for delta in &graphic.deltas {
            self.append_graphic(
                i32::from(delta.graphic_id),
                [
                    offset[0] + i32::from(delta.offset_x),
                    offset[1] + i32::from(delta.offset_y),
                ],
                misplacement,
                chain,
                out,
            )?;
        }
        chain.pop();
        Ok(())
    }
}
