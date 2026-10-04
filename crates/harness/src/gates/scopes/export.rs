use super::*;

pub(super) fn import_object(
    source: &Path,
    destination: &Path,
    object: &str,
    kind: &str,
) -> Result<()> {
    // Git provides object bytes, NOT worktree bytes; no checkout filters run here.
    let bytes = git(source, &["cat-file", kind, oid(object)?], None)?;
    let imported = line(git(
        destination,
        &["hash-object", "-w", "-t", kind, "--stdin"],
        Some(&bytes),
    )?)?;
    if imported != object {
        return Err("object identity changed during import".into());
    }
    Ok(())
}

pub(super) fn tree_records(root: &Path, commit: &str) -> Result<Vec<u8>> {
    // ls-tree output uses MODE TYPE OID<TAB>PATH; normalize to index-info.
    let listing = git(root, &["ls-tree", "-r", "--full-tree", "-z", commit], None)?;
    let mut records = Vec::new();
    for record in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("malformed tree record")?;
        let header: Vec<_> = std::str::from_utf8(&record[..tab])?.split(' ').collect();
        if header.len() != 3 {
            return Err("malformed tree header".into());
        }
        records.extend_from_slice(format!("{} {} 0\t", header[0], header[2]).as_bytes());
        records.extend_from_slice(&record[tab + 1..]);
        records.push(0);
    }
    Ok(records)
}

pub(super) fn prepare_checkout(
    source: &Path,
    probe: &Probe,
    kind: &Kind,
    parent: &Path,
) -> Result<(TempDir, PathBuf, String)> {
    let temporary = tempfile::Builder::new()
        .prefix("aoe-snapshot-")
        .tempdir_in(parent)?;
    let checkout = fs::canonicalize(temporary.path())?;
    let format = line(git(source, &["rev-parse", "--show-object-format"], None)?)?;
    if !matches!(format.as_str(), "sha1" | "sha256") {
        return Err("unknown Git object format".into());
    }
    git(
        &checkout,
        &[
            "init",
            "--quiet",
            "--template=",
            &format!("--object-format={format}"),
        ],
        None,
    )?;
    let (records, commit) = match kind {
        Kind::Index => (probe.entries.clone(), None),
        Kind::Commit(commit) => {
            oid(commit)?;
            if resolve(source, commit, "commit")? != *commit {
                return Err("commit identity mismatch".into());
            }
            let records = tree_records(source, commit)?;
            (records, Some(commit))
        }
        Kind::Working => return Err("working scope must not be exported".into()),
    };
    let parsed = entries(&records)?;
    let mut imported = BTreeSet::new();
    for entry in &parsed {
        if imported.insert(&entry.object) {
            import_object(source, &checkout, &entry.object, "blob")?;
        }
    }
    git(
        &checkout,
        &["update-index", "-z", "--index-info"],
        Some(&records),
    )?;
    let tree = line(git(&checkout, &["write-tree"], None)?)?;
    // Highest-priority attributes prevent text, filter, ident, and encoding edits.
    // Only PRIVATE metadata/config is written; no source refs/index/config mutate.
    fs::create_dir_all(checkout.join(".git/info"))?;
    fs::write(
        checkout.join(".git/info/attributes"),
        b"* -text -filter -ident -working-tree-encoding\n",
    )?;
    git(&checkout, &["config", "core.autocrlf", "false"], None)?;
    git(
        &checkout,
        &["config", "core.hooksPath", ".git/disabled-hooks"],
        None,
    )?;
    let detached = if let Some(commit) = commit {
        if tree != resolve(source, commit, "tree")? {
            return Err("commit tree import mismatch".into());
        }
        import_object(source, &checkout, commit, "commit")?;
        commit.to_owned()
    } else {
        let raw = format!(
            "tree {tree}\nauthor Snapshot <snapshot@example.invalid> 0 +0000\ncommitter Snapshot <snapshot@example.invalid> 0 +0000\n\nExact index snapshot (not a source commit).\n"
        );
        line(git(
            &checkout,
            &["hash-object", "-w", "-t", "commit", "--stdin"],
            Some(raw.as_bytes()),
        )?)?
    };
    // HEAD is genuinely detached and its TREE matches the private index.
    fs::write(checkout.join(".git/HEAD"), format!("{detached}\n"))?;
    let mut prefix = checkout.as_os_str().to_os_string();
    prefix.push(std::path::MAIN_SEPARATOR.to_string());
    let prefix = format!(
        "--prefix={}",
        prefix.to_str().ok_or("non-UTF-8 temporary path")?
    );
    git(&checkout, &["checkout-index", "--all", &prefix], None)?;
    verify_checkout(&checkout, &tree)?;
    Ok((temporary, checkout, tree))
}

pub(super) fn verify_checkout(checkout: &Path, tree: &str) -> Result<()> {
    metadata_seal(checkout, None)?;
    if resolve(checkout, "HEAD", "tree")? != tree {
        return Err("gate altered snapshot HEAD tree".into());
    }
    let mut index_entries = entries(&git(checkout, &["ls-files", "--stage", "-z"], None)?)?;
    let mut head_entries = entries(&tree_records(checkout, "HEAD")?)?;
    index_entries.sort_by(|a, b| a.name.cmp(&b.name));
    head_entries.sort_by(|a, b| a.name.cmp(&b.name));
    if index_entries != head_entries {
        return Err("gate altered snapshot index tree".into());
    }
    // Compare actual bytes to blob OIDs, not a filtered Git diff alone.
    for entry in index_entries {
        let path = checked_file(checkout, &entry.name)?;
        let actual = line(git(
            checkout,
            &["hash-object", "--no-filters", "--stdin"],
            Some(&fs::read(&path)?),
        )?)?;
        if actual != entry.object {
            return Err(format!("export altered {}", entry.name).into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if (fs::metadata(path)?.permissions().mode() & 0o111 != 0) != (entry.mode == "100755") {
                return Err("executable mode mismatch".into());
            }
        }
    }
    Ok(())
}
