//! Validate the full native actor timelines selected by the current importer.
use crate::{
    Error,
    catalog::units::{Pose, SPRITES},
    dat::de_survey,
};
use std::path::Path;

pub(super) fn validate_dat(install: &Path) -> Result<(), Error> {
    let dat = super::read_bounded(&install.join("resources/_common/dat/empires2_x2_p1.dat"))?;
    let bytes = de_survey::inflate(&dat)?;
    for pose in [
        Pose::GeneralStand,
        Pose::GeneralWalk,
        Pose::GeneralAttack,
        Pose::GeneralDie,
        Pose::GeneralDecay,
        Pose::Stone,
    ] {
        let source = SPRITES[pose as usize];
        let file = source.manifest_source();
        let name = file
            .strip_prefix("de/")
            .and_then(|name| name.strip_suffix(".sld"))
            .ok_or_else(|| crate::invalid("DE actor", 0, "unsupported native source identity"))?;
        let graphic = de_survey::graphics::find_sprite(&bytes, name, source.id as i32)?;
        if graphic.frames != u16::from(source.stride)
            || usize::from(graphic.angles) != source.rows()
            || (graphic.seconds * 1000.0).round() != f32::from(source.millis)
            || (f64::from(graphic.seconds) * 1000.0 - source.frame_millis()).abs() > 0.00001
            || graphic.replay_delay != 0.0
        {
            return Err(crate::invalid(
                "DE actor",
                0,
                "DAT animation differs from runtime catalog",
            ));
        }
    }
    Ok(())
}
