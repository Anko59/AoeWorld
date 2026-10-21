//! `empires2.dat` decoder: raw DEFLATE, then the genie data layout.
//!
//! Parsed in order: version, terrain restrictions, player colours, sounds and
//! the graphics table. The terrain block, random maps and effects are skipped
//! by anchoring on the civilisation block, which is validated structurally.
//! Unit headers (tasks) are found as the one offset whose walk ends exactly at
//! that anchor. Only the first civilisation (Gaia, the default unit
//! set) is decoded. Every count is bounded before allocation and the report
//! names where each stage began, so a layout drift is visible, not silent.
mod building;
mod composition;
pub mod de_survey;
mod reader;
mod sections;
mod terrain;
pub use building::{Annex, Building};
pub use composition::{BuildingComposition, BuildingPart};
pub use terrain::Terrain;
pub mod table;
mod table_details;
#[cfg(test)]
mod tests;
mod units;

use crate::Error;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;

/// Versions whose section sizes have been measured on real data.
pub const SUPPORTED_VERSIONS: &[&str] = &["VER 5.7"];
const MAX_INFLATED: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Graphic {
    pub byte_offset: usize,
    pub id: i32,
    pub name: String,
    pub file_name: String,
    pub slp_id: i32,
    pub frame_count: u16,
    pub angle_count: u16,
    pub frame_rate: f32,
    pub replay_delay: f32,
    pub layer: u8,
    pub deltas: Vec<GraphicDelta>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphicDelta {
    pub graphic_id: i16,
    pub offset_x: i16,
    pub offset_y: i16,
    pub display_angle: i16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Task {
    pub kind: i16,
    pub id: i16,
    pub action: i16,
    pub class: i16,
    pub unit: i16,
    pub terrain: i16,
    pub resource_in: i16,
    pub resource_out: i16,
    pub work_values: [f32; 3],
    /// Moving, proceeding, working, carrying (graphic ids, -1 when absent).
    pub graphics: [i16; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Cost {
    pub resource: i16,
    pub amount: i16,
    pub enabled: i16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Unit {
    pub id: i32,
    /// Genie unit type: 10 base, 20/25 moving, 30 dead, 60 projectile, 70 creatable, 80 building.
    pub kind: u8,
    pub name: String,
    pub hit_points: i16,
    pub line_of_sight: f32,
    pub collision_size: [f32; 2],
    pub resource_capacity: i16,
    pub resource_storage: [(i16, f32); 3],
    pub work_rate: Option<f32>,
    pub standing: [i16; 2],
    /// Full collision footprint in tiles (the file stores half extents).
    pub footprint: [f32; 2],
    pub building: Option<Building>,
    pub dying: i16,
    pub dead_unit: i16,
    pub speed: Option<f32>,
    pub walking: Option<[i16; 2]>,
    pub attack_graphic: Option<i16>,
    pub attacks: Vec<(i16, i16)>,
    pub armours: Vec<(i16, i16)>,
    pub costs: Vec<Cost>,
    pub train_time: Option<i16>,
    pub train_location: Option<i16>,
    pub construction_graphic: Option<i16>,
    pub tasks: Vec<Task>,
}

/// Where each stage began and how far parsing got.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Report {
    pub inflated_bytes: usize,
    pub sounds: usize,
    pub graphics_offset: usize,
    pub graphics_end: usize,
    pub civilisation_offset: Option<usize>,
    pub civilisation_count: Option<usize>,
    pub civilisation_name: Option<String>,
    pub unit_headers_offset: Option<usize>,
    pub unit_slots: usize,
    pub units_parsed: usize,
    /// Why a later stage did not run, if it did not.
    pub stopped: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatFile {
    pub version: String,
    pub graphics: Vec<Graphic>,
    pub terrains: Vec<Terrain>,
    pub units: Vec<Unit>,
    pub report: Report,
    #[serde(skip)]
    graphic_index: HashMap<i32, usize>,
    #[serde(skip)]
    unit_index: HashMap<i32, usize>,
}

impl DatFile {
    pub fn graphic(&self, id: i32) -> Option<&Graphic> {
        self.graphic_index.get(&id).map(|at| &self.graphics[*at])
    }

    pub fn unit(&self, id: i32) -> Option<&Unit> {
        self.unit_index.get(&id).map(|at| &self.units[*at])
    }
}

/// Inflate the raw (header-less) DEFLATE stream and parse it.
pub fn decode(compressed: &[u8]) -> Result<DatFile, Error> {
    let mut inflated = Vec::new();
    flate2::read::DeflateDecoder::new(compressed)
        .take(MAX_INFLATED + 1)
        .read_to_end(&mut inflated)?;
    if inflated.len() as u64 > MAX_INFLATED {
        return Err(crate::invalid(
            reader::FORMAT,
            0,
            "inflated data exceeds 64 MiB",
        ));
    }
    parse(&inflated)
}

/// Parse already-inflated bytes. The graphics table is mandatory; a later
/// stage that cannot be validated stops with a reason in the report.
pub fn parse(bytes: &[u8]) -> Result<DatFile, Error> {
    let mut r = reader::Reader::new(bytes, 0);
    let version = sections::version(&mut r)?;
    let sounds = sections::skip_to_graphics(&mut r)?;
    let graphics_offset = r.pos;
    let graphics = sections::graphics(&mut r)?;
    let graphics_end = r.pos;
    let terrains = terrain::decode(&mut r)?;
    let mut report = Report {
        inflated_bytes: bytes.len(),
        sounds,
        graphics_offset,
        graphics_end,
        ..Report::default()
    };
    let units = match units::decode(bytes, r.pos, &mut report) {
        Ok(units) => units,
        Err(error) => {
            report.stopped = Some(error.to_string());
            Vec::new()
        }
    };
    let graphic_index = graphics
        .iter()
        .enumerate()
        .map(|(at, g)| (g.id, at))
        .collect();
    let unit_index = units.iter().enumerate().map(|(at, u)| (u.id, at)).collect();
    Ok(DatFile {
        version,
        graphics,
        terrains,
        units,
        report,
        graphic_index,
        unit_index,
    })
}
