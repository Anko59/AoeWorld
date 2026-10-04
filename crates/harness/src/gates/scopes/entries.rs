use super::*;

pub(super) fn relative(name: &str) -> Result<()> {
    // Portable fail-closed policy also rejects Windows separators/drives/ADS.
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git")
        })
        || Path::new(name)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("unsafe snapshot path {name:?}").into());
    }
    Ok(())
}

pub(super) fn checked_file(root: &Path, name: &str) -> Result<PathBuf> {
    relative(name)?;
    let mut path = root.to_owned();
    let components: Vec<_> = Path::new(name).components().collect();
    for (index, component) in components.iter().enumerate() {
        path.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&path)?;
        let last = index + 1 == components.len();
        if (last && !metadata.file_type().is_file()) || (!last && !metadata.file_type().is_dir()) {
            return Err(format!("unsafe snapshot file or ancestor: {name:?}").into());
        }
    }
    Ok(path)
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Entry {
    pub(super) mode: String,
    pub(super) object: String,
    pub(super) name: String,
}

pub(super) fn entries(bytes: &[u8]) -> Result<Vec<Entry>> {
    bytes
        .split(|b| *b == 0)
        .filter(|entry| !entry.is_empty())
        .map(|record| {
            let (header, name) = record.split_at(
                record
                    .iter()
                    .position(|b| *b == b'\t')
                    .ok_or("malformed index record")?,
            );
            let fields: Vec<_> = std::str::from_utf8(header)?.split(' ').collect();
            if fields.len() != 3 || fields[2] != "0" {
                return Err("unmerged or malformed index: resolve conflicts first".into());
            }
            // Reject ALL symlinks and gitlinks, including apparently benign ones.
            // No extractor ever follows candidate-controlled links or submodules.
            if !matches!(fields[0], "100644" | "100755") {
                return Err(
                    "snapshot v1 supports regular files only (no symlinks/gitlinks)".into(),
                );
            }
            oid(fields[1])?;
            let name = std::str::from_utf8(&name[1..])?.to_owned();
            relative(&name)?;
            Ok(Entry {
                mode: fields[0].to_owned(),
                object: fields[1].to_owned(),
                name,
            })
        })
        .collect()
}
