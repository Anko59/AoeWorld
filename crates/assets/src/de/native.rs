//! Verify generated metadata against the live licensed DAT before importing.
use crate::{Error, catalog::units::SPRITES};
pub(super) fn validate_catalog(selected: &[super::unit_catalog::Selected]) -> Result<(), Error> {
    for g in selected {
        let source = SPRITES
            .get(g.pose)
            .ok_or_else(|| crate::invalid("DE unit", 0, "regenerate unit catalog"))?;
        if source.id != g.sprite
            || u16::from(source.stride) != if g.terminal { 1 } else { g.frames }
            || source.rows() != usize::from(g.angles)
            || source.frames() != u32::from(g.cells)
            || source.frame_millis()
                != if g.terminal {
                    0.0
                } else {
                    f64::from(g.seconds) * 1000.0
                }
            || source.replay_delay_seconds() != g.replay
        {
            return Err(crate::invalid(
                "DE unit",
                0,
                "DAT differs from catalog; run make assets-unit-table-de",
            ));
        }
    }
    Ok(())
}
