//! Local-only attachment of a DE pack to the selected trial pack.
use std::path::Path;

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
            if link.canonicalize()? == de {
                return Ok(());
            }
            return Err("building overlay already points to another pack".into());
        }
        Ok(_) => return Err("building overlay path is already occupied".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::os::unix::fs::symlink(&de, &link)?;
    println!("attached building pack to {}", base.display());
    Ok(())
}
