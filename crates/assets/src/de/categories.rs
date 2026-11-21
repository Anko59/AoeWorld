//! One current DE pack layout: bounded world and actor categories, both required.
use super::*;
use crate::dat::{BuildingComposition, BuildingDamage};

/// Keep the world and animated-effect recipes under their own startup budgets.
pub(super) fn split_fires(recipes: &mut [BuildingComposition]) -> Vec<BuildingComposition> {
    let mut fires = Vec::new();
    for building in recipes {
        let mut damage = Vec::new();
        for state in &mut building.damage {
            let mut parts = Vec::new();
            state.parts.retain(|part| {
                if part.source.starts_with("de/particles/") {
                    parts.push(part.clone());
                    false
                } else {
                    true
                }
            });
            if !parts.is_empty() {
                damage.push(BuildingDamage {
                    damage_percent: state.damage_percent,
                    apply_mode: state.apply_mode,
                    parts,
                });
            }
        }
        if !damage.is_empty() {
            fires.push(BuildingComposition {
                unit_id: building.unit_id,
                footprint: building.footprint,
                foundation_terrain: -1,
                foundation_slp: None,
                foundation_size: [0; 2],
                parts: Vec::new(),
                construction: Vec::new(),
                states: Vec::new(),
                damage,
            });
        }
    }
    fires
}

pub(super) fn indices(
    len: usize,
    actors: std::ops::Range<usize>,
) -> Result<(Vec<usize>, Vec<usize>), Error> {
    if actors.start > actors.end || actors.end > len {
        return Err(crate::invalid(
            "DE categories",
            0,
            "invalid source partition",
        ));
    }
    Ok((
        (0..actors.start).chain(actors.end..len).collect(),
        actors.collect(),
    ))
}

pub(super) fn attach(world: &Path, actors: &Path) -> Result<(), Error> {
    #[cfg(not(unix))]
    {
        let _ = (world, actors);
        Err(crate::invalid(
            "DE categories",
            0,
            "local category links require Unix",
        ))
    }
    #[cfg(unix)]
    {
        let actors = actors.canonicalize()?;
        let link = world.join("actors");
        match fs::symlink_metadata(&link) {
            Ok(m) if m.file_type().is_symlink() && link.canonicalize()? == actors => Ok(()),
            Ok(_) => Err(crate::invalid(
                "DE categories",
                0,
                "actor category link already occupied",
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::os::unix::fs::symlink(actors, link)?;
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests;
