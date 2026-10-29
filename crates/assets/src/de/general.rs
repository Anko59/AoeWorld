//! Normalize the mounted hero into the five facing cells the client draws.
//! Full native topology is validated before discarding unused source frames.
use crate::{Error, sld::Frame};

pub(super) fn sample(role: &str, frames: Vec<Frame>) -> Result<Vec<Frame>, Error> {
    let (count, period, last) = match role {
        "idle" => (721, 45, 0),
        "walk" => (480, 30, 0),
        "death" => (721, 45, 44),
        _ => return Err(crate::invalid("DE General", 0, "unsupported hero pose")),
    };
    if frames.len() != count {
        return Err(crate::invalid(
            "DE General",
            0,
            "unsupported native hero topology",
        ));
    }
    let cells = [4, 6, 8, 10, 12].map(|direction| direction * period + last);
    Ok(frames
        .into_iter()
        .enumerate()
        .filter_map(|(index, frame)| cells.contains(&index).then_some(frame))
        .collect())
}

#[cfg(test)]
mod tests;
