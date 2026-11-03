//! One current DE pack layout: bounded world and actor categories, both required.
use super::*;

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
