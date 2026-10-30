//! Targeted current unit records, without decoding unrelated effects or research.
use super::{Building, Error, SOLDIERS, unit_record};

pub fn soldier(bytes: &[u8], id: i16) -> Result<Building, Error> {
    let &(_, language) = SOLDIERS
        .iter()
        .find(|&&(unit, _)| unit == id)
        .ok_or_else(|| crate::invalid("DE survey", 0, "unreviewed soldier identity"))?;
    let mut signature = vec![70];
    signature.extend_from_slice(&id.to_le_bytes());
    signature.extend_from_slice(&language.to_le_bytes());
    let at = bytes
        .windows(signature.len())
        .position(|b| b == signature)
        .ok_or_else(|| crate::invalid("DE survey", 0, format!("soldier {id} not found")))?;
    unit_record(bytes, at, id, "")
}
