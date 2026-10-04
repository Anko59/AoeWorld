use super::*;

pub(super) fn index_fingerprint(root: &Path, index: &Path, entries: &[u8]) -> Result<String> {
    let mut hash = blake3::Hasher::new();
    hash.update(index.as_os_str().as_encoded_bytes());
    for bytes in [fs::read(index)?, entries.to_vec()] {
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    // Split indexes refer to an immutable shared index outside the main file.
    let shared = line(git_at(
        root,
        Some(index),
        &["rev-parse", "--shared-index-path"],
        None,
    )?)?;
    if !shared.is_empty() {
        let path = Path::new(&shared);
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            root.join(path)
        };
        let bytes = fs::read(source_index(root, Some(&path))?)?;
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    Ok(hash.finalize().to_hex().to_string())
}

pub(super) fn probe(root: &Path, index: &Path, working: bool) -> Result<Probe> {
    source_index(root, Some(index))?;
    let head = resolve(root, "HEAD", "commit")?;
    let entries = git_at(root, Some(index), &["ls-files", "--stage", "-z"], None)?;
    let fingerprint = index_fingerprint(root, index, &entries)?;
    let invisible = git_at(
        root,
        Some(index),
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--cached",
            "--no-renames",
            "--name-only",
            "-z",
            "--ita-invisible-in-index",
            &head,
            "--",
        ],
        None,
    )?;
    let visible = git_at(
        root,
        Some(index),
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--cached",
            "--no-renames",
            "--name-only",
            "-z",
            "--ita-visible-in-index",
            &head,
            "--",
        ],
        None,
    )?;
    if visible != invisible {
        return Err(
            "intent-to-add entries are not materialized index blobs; stage or remove them".into(),
        );
    }
    let mut paths: BTreeSet<_> = names(&invisible)?.into_iter().collect();
    let mut working_clean = false;
    let working = if working {
        paths.extend(names(&git_at(
            root,
            Some(index),
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--name-only",
                "-z",
                &head,
                "--",
            ],
            None,
        )?)?);
        let untracked = names(&git_at(
            root,
            Some(index),
            &["ls-files", "--others", "--exclude-standard", "-z"],
            None,
        )?)?;
        paths.extend(untracked.iter().cloned());
        let status = git_at(
            root,
            Some(index),
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            None,
        )?;
        working_clean = status.is_empty() && paths.is_empty();
        let mut hash = blake3::Hasher::new();
        hash.update(&status);
        // Raw probes catch edits whose status remains M and stat/assume-unchanged
        // shortcuts. Ignored caches are deliberately not source identity inputs.
        let mut inspect = |name: &str, expected: Option<&Entry>| -> Result<()> {
            relative(name)?;
            hash.update(&(name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            match checked_file(root, name) {
                Ok(path) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let executable = fs::metadata(&path)?.permissions().mode() & 0o111 != 0;
                        hash.update(&[u8::from(executable)]);
                        if expected.is_some_and(|entry| executable != (entry.mode == "100755")) {
                            working_clean = false;
                            paths.insert(name.to_owned());
                        }
                    }
                    let bytes = fs::read(path)?;
                    hash.update(&(bytes.len() as u64).to_le_bytes());
                    hash.update(&bytes);
                    if let Some(entry) = expected {
                        let actual = line(git_at(
                            root,
                            Some(index),
                            &["hash-object", "--no-filters", "--stdin"],
                            Some(&bytes),
                        )?)?;
                        if actual != entry.object {
                            working_clean = false;
                            paths.insert(name.to_owned());
                        }
                    }
                }
                Err(error) => {
                    let absent = error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound);
                    if absent {
                        hash.update(b"missing");
                        working_clean = false;
                        paths.insert(name.to_owned());
                    } else {
                        return Err(error);
                    }
                }
            }
            Ok(())
        };
        for entry in self::entries(&entries)? {
            inspect(&entry.name, Some(&entry))?;
        }
        for name in &untracked {
            inspect(name, None)?;
        }
        Some(hash.finalize().as_bytes().to_vec())
    } else {
        None
    };
    Ok(Probe {
        head,
        entries,
        fingerprint,
        paths: paths.into_iter().collect(),
        working,
        working_clean,
    })
}
