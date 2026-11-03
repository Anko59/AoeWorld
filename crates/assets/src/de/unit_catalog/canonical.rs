//! Share only identical resolved native drawing recipes, never unit identities.
use super::*;

pub(super) fn apply(selected: &mut [Selected]) -> Result<(), Error> {
    let mut sources = BTreeMap::new();
    for s in selected {
        // Graphic/SLP IDs do not affect decoded pixels once a concrete installed
        // filename is resolved. Part order, offsets and every native timing do.
        let parts: Vec<_> = s
            .parts
            .iter()
            .map(|p| {
                (
                    &p.animation.file,
                    p.animation.frames,
                    p.animation.angles,
                    p.animation.seconds.to_bits(),
                    p.animation.replay_delay.to_bits(),
                    p.x,
                    p.y,
                    p.layer,
                )
            })
            .collect();
        let key = serde_json::to_string(&(
            s.frames,
            s.angles,
            s.cells,
            s.seconds.to_bits(),
            s.replay.to_bits(),
            parts,
        ))?;
        if let Some((sprite, file)) = sources.get(&key) {
            s.sprite = *sprite;
            s.file = String::clone(file);
        } else {
            sources.insert(key, (s.sprite, s.file.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
