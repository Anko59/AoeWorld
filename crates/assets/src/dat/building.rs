//! AoK VER 5.7's 80-byte building tail. Unlike TC/DE, AoK has no snow slot.
use super::reader::Reader;
use crate::Error;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Annex {
    pub unit_id: i16,
    pub misplacement: [f32; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Building {
    pub byte_offset: usize,
    pub construction_graphic: i16,
    pub adjacent_mode: u8,
    pub graphics_angle: i16,
    pub disappears_when_built: u8,
    pub stack_unit: i16,
    pub foundation_terrain: i16,
    pub old_terrain: i16,
    pub research: i16,
    pub can_burn: u8,
    pub annexes: [Annex; 4],
    pub head_unit: i16,
    pub transform_unit: i16,
    pub transform_sound: i16,
    pub construction_sound: i16,
    pub garrison_type: u8,
    pub garrison_heal_rate: f32,
    pub garrison_repair_rate: f32,
    pub pile_unit: i16,
    pub looting: [u8; 6],
}

pub(super) fn decode(r: &mut Reader) -> Result<Building, Error> {
    let byte_offset = r.pos;
    let construction_graphic = r.i16()?;
    let adjacent_mode = r.u8()?;
    let graphics_angle = r.i16()?;
    let disappears_when_built = r.u8()?;
    let stack_unit = r.i16()?;
    let foundation_terrain = r.i16()?;
    let old_terrain = r.i16()?;
    let research = r.i16()?;
    let can_burn = r.u8()?;
    let mut annexes = [Annex {
        unit_id: -1,
        misplacement: [0.0; 2],
    }; 4];
    for annex in &mut annexes {
        annex.unit_id = r.i16()?;
        annex.misplacement = [r.f32()?, r.f32()?];
        if annex
            .misplacement
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 128.0)
        {
            return Err(r.fail("invalid building annex misplacement"));
        }
    }
    let head_unit = r.i16()?;
    let transform_unit = r.i16()?;
    let transform_sound = r.i16()?;
    let construction_sound = r.i16()?;
    let garrison_type = r.u8()?;
    let garrison_heal_rate = r.f32()?;
    let garrison_repair_rate = r.f32()?;
    let pile_unit = r.i16()?;
    let mut looting = [0; 6];
    looting.copy_from_slice(r.take(6)?);
    debug_assert_eq!(r.pos - byte_offset, 80);
    Ok(Building {
        byte_offset,
        construction_graphic,
        adjacent_mode,
        graphics_angle,
        disappears_when_built,
        stack_unit,
        foundation_terrain,
        old_terrain,
        research,
        can_burn,
        annexes,
        head_unit,
        transform_unit,
        transform_sound,
        construction_sound,
        garrison_type,
        garrison_heal_rate,
        garrison_repair_rate,
        pile_unit,
        looting,
    })
}
