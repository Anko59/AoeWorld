use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub path: String,
    pub mode: String,
    pub blob: String,
    pub blake3: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Closure {
    pub schema: u32,
    pub source: SourceIdentity,
    /// Whole tracked protected repository, sorted by root-relative UTF-8 path.
    /// Conservative superset includes all judge Cargo dependency code, build
    /// configuration, lockfiles, registry/Make/hook/workflow/schema/instructions.
    pub entries: Vec<Entry>,
    pub blake3: String,
}
impl Closure {
    pub(crate) fn bind(source: SourceIdentity, entries: Vec<Entry>) -> Result<Self> {
        full_oid(&source.commit)?;
        full_oid(&source.tree)?;
        if entries.is_empty() {
            return Err("empty protected trust closure".into());
        }
        let mut previous: Option<&str> = None;
        for entry in &entries {
            if previous.is_some_and(|previous| previous >= entry.path.as_str()) {
                return Err("duplicate or unordered closure path".into());
            }
            if !Path::new(&entry.path)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
                || entry.path.is_empty()
            {
                return Err("invalid closure path".into());
            }
            if !matches!(entry.mode.as_str(), "100644" | "100755") {
                return Err("policy tree symlink/gitlink/special mode is unsupported".into());
            }
            full_oid(&entry.blob)?;
            digest(&entry.blake3)?;
            previous = Some(&entry.path);
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-protected-policy-whole-tree-v1\0");
        hash.update(&serde_json::to_vec(&(
            &source.repository_id,
            &source.repository,
            &source.remote_url,
            &source.protected_ref,
            &source.commit,
            &source.tree,
            &entries,
        ))?);
        Ok(Self {
            schema: 1,
            source,
            entries,
            blake3: hash.finalize().to_hex().to_string(),
        })
    }
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let closure: Self = serde_json::from_slice(bytes)?;
        if closure.schema != 1 {
            return Err("unsupported closure schema".into());
        }
        let expected = Self::bind(closure.source.clone(), closure.entries.clone())?;
        if expected.blake3 != closure.blake3 {
            return Err("tampered policy bundle manifest".into());
        }
        Ok(closure)
    }
}

pub(crate) struct Materialized {
    root: PathBuf,
    closure: Closure,
}
impl Materialized {
    /// Closure and complete entry inventory originate in freshly fetched PRIVATE
    /// Git object metadata, not a caller-supplied manifest. Parent backend MUST
    /// compare commit tree records exactly against entries before constructing.
    pub(super) fn from_snapshot(
        snapshot: &crate::gates::scopes::Snapshot,
        source: SourceIdentity,
    ) -> Result<Self> {
        use crate::gates::scopes::Kind;
        if snapshot.identity.kind != Kind::Commit(source.commit.clone())
            || snapshot.identity.tree.as_ref() != Some(&source.tree)
            || snapshot.identity.source_head != source.commit
        {
            return Err("protected source identity differs from freshly fetched snapshot".into());
        }
        snapshot.run_checked(|root| {
            let entries = snapshot
                .inventory()?
                .into_iter()
                .map(|(path, mode, blob)| {
                    let bytes = fs::read(regular(root, &path)?)?;
                    Ok(Entry {
                        path,
                        mode,
                        blob,
                        blake3: blake3::hash(&bytes).to_hex().to_string(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Self::new(root, Closure::bind(source, entries)?)
        })
    }
    pub(super) fn new(root: &Path, closure: Closure) -> Result<Self> {
        let root = plain_absolute(root)?;
        let materialized = Self { root, closure };
        materialized.verify()?;
        Ok(materialized)
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn closure(&self) -> &Closure {
        &self.closure
    }
    pub(crate) fn verify(&self) -> Result<()> {
        // Recomputed checksum alone is consistency, never authenticity.
        Closure::parse(&serde_json::to_vec(&self.closure)?)?;
        let mut inventory = BTreeSet::new();
        fn visit(root: &Path, directory: &Path, inventory: &mut BTreeSet<String>) -> Result<()> {
            for child in fs::read_dir(directory)? {
                let child = child?.path();
                let relative = child
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF-8 policy path")?;
                // Private Git metadata must be sealed independently by Snapshot;
                // no other untracked file (e.g. .cargo/config) can shadow closure.
                let metadata = fs::symlink_metadata(&child)?;
                if relative == ".git" {
                    if !metadata.file_type().is_dir() {
                        return Err("private Git metadata is not a real directory".into());
                    }
                    continue;
                }
                if metadata.file_type().is_dir() {
                    visit(root, &child, inventory)?;
                } else if metadata.file_type().is_file() {
                    inventory.insert(relative.to_owned());
                } else {
                    return Err("policy materialization contains symlink/special path".into());
                }
            }
            Ok(())
        }
        visit(&self.root, &self.root, &mut inventory)?;
        if inventory
            != self
                .closure
                .entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect()
        {
            return Err("untracked or missing policy input invalidates complete closure".into());
        }
        for entry in &self.closure.entries {
            let path = regular(&self.root, &entry.path)?;
            if blake3::hash(&fs::read(&path)?).to_hex().as_str() != entry.blake3 {
                return Err(format!("modified protected policy file {}", entry.path).into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if (fs::metadata(&path)?.permissions().mode() & 0o111 != 0)
                    != (entry.mode == "100755")
                {
                    return Err("protected policy executable mode changed".into());
                }
            }
        }
        Ok(())
    }
}

// Checksums are consistency, not verified artifact attestation or identity.
