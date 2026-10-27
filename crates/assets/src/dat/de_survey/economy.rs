//! Villager professions share the DE creatable-unit layout. These are numeric
//! facts only; the licensed bytes and the generated survey remain local.
use super::{Building, Error, unit_record};

pub(super) const WORKERS: &[(i16, i32, &str)] = &[
    (120, 5402, "VMFOR"),
    (122, 5124, "VMHUN"),
    (123, 5125, "VMLUM"),
    (124, 5126, "VMMIN"),
    (214, 5123, "VFFAR"),
    (259, 5123, "VMFAR"),
    (579, 5482, "VMGLD"),
    (581, 5482, "VFGLD"),
    (590, 5496, "VFSHE"),
    (592, 5496, "VMSHE"),
];

pub(super) fn workers(bytes: &[u8]) -> Result<Vec<Building>, Error> {
    WORKERS
        .iter()
        .map(|&(id, language, name)| {
            let mut signature = vec![70];
            signature.extend_from_slice(&id.to_le_bytes());
            signature.extend_from_slice(&language.to_le_bytes());
            let at = bytes
                .windows(signature.len())
                .position(|b| b == signature)
                .ok_or_else(|| crate::invalid("DE survey", 0, format!("worker {id} not found")))?;
            unit_record(bytes, at, id, name)
        })
        .collect()
}
