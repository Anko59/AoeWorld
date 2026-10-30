//! Current DE building tails; all roles and annex positions stay data-driven.
use super::*;
#[derive(Clone, Debug, Serialize)]
pub struct Damage {
    pub graphic: i16,
    pub percent: i16,
    pub mode: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct Annex {
    pub unit: i16,
    pub offset: [f32; 2],
}
#[derive(Clone, Debug, Serialize)]
pub struct Building {
    pub construction: i16,
    pub snow: i16,
    pub destruction: i16,
    pub rubble: i16,
    pub researching: i16,
    pub research_complete: i16,
    pub foundation: i16,
    pub annexes: Vec<Annex>,
}
pub(super) fn parse(r: &mut Reader<'_>) -> Result<Building, Error> {
    r.skip(32 + 18)?;
    let trains = usize::from(r.u16()?);
    r.bounded(trains, 256, "train locations")?;
    r.skip(trains * 9 + 97)?;
    let construction = r.i16()?;
    let snow = r.i16()?;
    let destruction = r.i16()?;
    let rubble = r.i16()?;
    let researching = r.i16()?;
    let research_complete = r.i16()?;
    r.skip(1 + 2 + 1 + 2)?;
    let foundation = r.i16()?;
    r.skip(2 + 2 + 1)?;
    let mut annexes = Vec::new();
    for _ in 0..4 {
        let unit = r.i16()?;
        let offset = [r.f32()?, r.f32()?];
        if offset.iter().any(|n| !n.is_finite() || n.abs() > 128.0) {
            return Err(r.fail("annex offset"));
        }
        if unit >= 0 {
            annexes.push(Annex { unit, offset });
        }
    }
    r.skip(33)?;
    Ok(Building {
        construction,
        snow,
        destruction,
        rubble,
        researching,
        research_complete,
        foundation,
        annexes,
    })
}
