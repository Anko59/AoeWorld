//! Moving projectile headers share the VER 8.9 base-unit layout.
use super::{Building, Error, Reader, debug};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ProjectileSource {
    pub id: i16,
    pub offset: usize,
    pub name: String,
    pub graphic: i16,
    pub speed: f32,
}
pub(super) fn projectile_record(
    bytes: &[u8],
    at: usize,
    id: i16,
) -> Result<ProjectileSource, Error> {
    let mut r = Reader::new(bytes, at);
    if r.u8()? != 60 || r.i16()? != id {
        return Err(r.fail("wrong projectile identity"));
    }
    r.skip(10)?;
    let graphic = r.i16()?;
    r.pos = at + 157;
    let n = usize::from(r.u8()?);
    r.bounded(n, 32, "damage graphics")?;
    r.skip(n * 5 + 22)?;
    let name = debug(&mut r)?;
    if r.i16()? != id || r.i16()? < 0 {
        return Err(r.fail("projectile copy/base identity disagrees"));
    }
    let speed = r.f32()?;
    if graphic < 0 || !speed.is_finite() || !(0.01..=100.0).contains(&speed) {
        return Err(r.fail("invalid projectile graphic/speed"));
    }
    Ok(ProjectileSource {
        id,
        offset: at,
        name,
        graphic,
        speed,
    })
}
pub(super) fn projectiles(
    bytes: &[u8],
    soldiers: &[Building],
    buildings: &[Building],
) -> Result<Vec<ProjectileSource>, Error> {
    let ids = soldiers
        .iter()
        .chain(buildings)
        .map(|s| s.projectile)
        .filter(|id| *id >= 0)
        .collect::<std::collections::BTreeSet<_>>();
    ids.into_iter()
        .map(|id| {
            let signature = [60, id.to_le_bytes()[0], id.to_le_bytes()[1]];
            bytes
                .windows(3)
                .enumerate()
                .filter(|(_, b)| *b == signature)
                .find_map(|(at, _)| projectile_record(bytes, at, id).ok())
                .ok_or_else(|| crate::invalid("DE survey", 0, format!("projectile {id} not found")))
        })
        .collect()
}
