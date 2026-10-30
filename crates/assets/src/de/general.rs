//! Preserve the hero's complete native topology, including every direction.
use crate::{Error, sld::Frame};

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
