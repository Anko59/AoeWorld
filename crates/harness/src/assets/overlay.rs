//! Local-only attachment of a DE pack to the selected trial pack.
use std::path::{Path, PathBuf};

/// The pack an import should fill when the caller did not name one: the only
/// existing pack that still has no building overlay. Several candidates stay
/// untouched because importing is not the place to choose the player's pack.
pub(super) fn default_base(
    packs: &Path,
    de: &Path,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let packs = packs.canonicalize()?;
    let de = de.canonicalize()?;
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&packs)? {
        let path = entry?.path();
        if path == de
            || !path.join("manifest.json").is_file()
            || std::fs::symlink_metadata(path.join("buildings")).is_ok()
        {
            continue;
        }
        candidates.push(path);
    }
    candidates.sort();
    Ok((candidates.len() == 1).then(|| candidates.remove(0)))
}

pub(super) fn attach(base: &Path, de: &Path) -> Result<(), Box<dyn std::error::Error>> {
    attach_in(Path::new("local-assets/packs"), base, de)
}

pub(super) fn attach_in(
    packs: &Path,
    base: &Path,
    de: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let packs = packs.canonicalize()?;
    let base = base.canonicalize()?;
    let de = de.canonicalize()?;
    if base.parent() != Some(packs.as_path())
        || de.parent() != Some(packs.as_path())
        || base == de
        || !base.join("manifest.json").is_file()
        || !de.join("manifest.json").is_file()
    {
        return Err("building overlay requires two distinct local-assets/packs children".into());
    }
    let link = base.join("buildings");
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if link.canonicalize().is_ok_and(|current| current == de) {
                return Ok(());
            }
            // The generated pointer is the current overlay, including after
            // an import changes its content hash. Only this local link is
            // replaced; neither the old target nor an occupied file is touched.
            std::fs::remove_file(&link)?;
        }
        Ok(_) => return Err("building overlay path is already occupied".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::os::unix::fs::symlink(&de, &link)?;
    println!("attached building pack to {}", base.display());
    Ok(())
}
