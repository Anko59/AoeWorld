//! Preserve the hero's complete native topology, including every direction.
use crate::{Error, sld::Frame};

pub(super) fn validate_dat(install: &std::path::Path) -> Result<(), Error> {
    use crate::{
        catalog::units::{Pose, SPRITES},
        dat::de_survey,
    };
    let dat = super::read_bounded(&install.join("resources/_common/dat/empires2_x2_p1.dat"))?;
    let bytes = de_survey::inflate(&dat)?;
    for pose in [
        Pose::GeneralStand,
        Pose::GeneralWalk,
        Pose::GeneralAttack,
        Pose::GeneralDie,
        Pose::GeneralDecay,
    ] {
        let s = SPRITES[pose as usize];
        let file = s.manifest_source();
        let name = file
            .strip_prefix("de/")
            .unwrap_or(&file)
            .strip_suffix(".sld")
            .unwrap_or(&file);
        let g = de_survey::graphics::find(&bytes, name)?;
        if g.sprite != s.id as i32
            || g.frames != u16::from(s.stride)
            || g.angles != 16
            || (g.seconds * 1000.0).round() != f32::from(s.millis)
            || (f64::from(g.seconds) * 1000.0 - s.frame_millis()).abs() > 0.00001
            || g.replay_delay != 0.0
        {
            return Err(crate::invalid(
                "DE General",
                0,
                "DAT animation differs from runtime catalog",
            ));
        }
    }
    Ok(())
}

pub(super) fn sample(role: &str, frames: Vec<Frame>) -> Result<Vec<Frame>, Error> {
    let count = match role {
        "idle" | "attack" | "death" => 721,
        "walk" | "decay" => 481,
        _ => return Err(crate::invalid("DE General", 0, "unsupported hero pose")),
    };
    if frames.len() != count {
        return Err(crate::invalid(
            "DE General",
            0,
            "unsupported native hero topology",
        ));
    }
    Ok(frames)
}

#[cfg(test)]
mod tests;
