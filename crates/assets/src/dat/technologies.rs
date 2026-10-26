//! AoK research table following the civilisation records. Layout reference:
//! Tapsa/genieutils src/dat/Research.cpp, AoK civilisation/full-tech fields.
use super::reader::{Reader, text};
use crate::Error;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Technology {
    pub id: i32,
    pub name: String,
    pub icon_id: i16,
    pub location: i16,
}

pub(super) fn decode(bytes: &[u8], at: usize) -> Result<Vec<Technology>, Error> {
    if at == bytes.len() {
        return Ok(Vec::new());
    }
    let mut r = Reader::new(bytes, at);
    let count = usize::from(r.u16()?);
    r.bounded(count, 4096, "technologies")?;
    let mut out = Vec::with_capacity(count);
    for id in 0..count {
        r.skip(12 + 15 + 2 + 4)?; // requirements, costs, required count, civ/full-tech flags
        let location = r.i16()?;
        r.skip(4 + 2 + 2 + 2)?; // language strings, time, effect, type
        let icon_id = r.i16()?;
        r.skip(1 + 12)?; // grid button, help/tree/hotkey strings
        let len = usize::from(r.u16()?);
        r.bounded(
            len,
            128,
            &format!("technology name (table at {at}, slot {id}, count {count})"),
        )?;
        let name = text(r.take(len)?);
        out.push(Technology {
            id: id as i32,
            name,
            icon_id,
            location,
        });
    }
    Ok(out)
}
