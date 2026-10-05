//! Stable logical subjects, not physical snapshot observations or authority.
//! Selected tree closure excludes ancestor-history contents. Existing endpoint
//! probes remain mandatory; local Git/filesystem IO is not hard wall-supervised.
use super::*;
use std::{collections::BTreeMap, io::Read};

#[cfg(test)]
mod tests;

const ALGORITHM: &str = "blake3:aoeworld-content-witness-v1";
const DOMAIN: &[u8] = b"aoeworld:content-witness:v1\0";
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "scope", rename_all = "kebab-case")]
pub(crate) enum ContentKind {
    Commit {
        commit: String,
        tree: String,
    },
    Index {
        source_head: String,
        pending_tree: String,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) enum FileMode {
    #[serde(rename = "100644")]
    Regular,
    #[serde(rename = "100755")]
    Executable,
}
impl FileMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "100644" => Ok(Self::Regular),
            "100755" => Ok(Self::Executable),
            _ => Err("content witness supports regular/executable files only".into()),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ObjectKind {
    Commit,
    Tree,
    Blob,
}
impl ObjectKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "commit" => Ok(Self::Commit),
            "tree" => Ok(Self::Tree),
            "blob" => Ok(Self::Blob),
            _ => Err("unsupported content object kind".into()),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ContentFile {
    pub(crate) path: String,
    pub(crate) mode: FileMode,
    pub(crate) blob_oid: String,
    pub(crate) raw_blake3: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ContentObject {
    pub(crate) oid: String,
    pub(crate) kind: ObjectKind,
    pub(crate) raw_blake3: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ContentWitness {
    pub(crate) version: u16,
    pub(crate) algorithm: String,
    pub(crate) kind: ContentKind,
    pub(crate) files: Vec<ContentFile>,
    pub(crate) objects: Vec<ContentObject>,
    pub(crate) digest: String,
}
// Compact typed field-order serialization with sorted vectors, not RFC canonical
// JSON. The generated digest itself is deliberately absent from this payload.
#[derive(Serialize)]
struct Payload<'a> {
    version: u16,
    algorithm: &'a str,
    kind: &'a ContentKind,
    files: &'a [ContentFile],
    objects: &'a [ContentObject],
}
fn add_object(root: &Path, objects: &mut Objects, object: &str, kind: &str) -> Result<()> {
    oid(object)?;
    ObjectKind::parse(kind)?;
    let bytes = git(root, &["cat-file", kind, object], None)?;
    let key = (object.to_owned(), kind.to_owned());
    let hash = blake3::hash(&bytes).to_hex().to_string();
    if objects.get(&key).is_some_and(|previous| previous != &hash) {
        return Err("raw content object changed during witness construction".into());
    }
    objects.insert(key, hash);
    Ok(())
}
fn tree_objects(root: &Path, tree: &str) -> Result<Objects> {
    let mut objects = BTreeMap::new();
    add_object(root, &mut objects, tree, "tree")?;
    let listing = git(
        root,
        &["ls-tree", "-r", "-t", "--full-tree", "-z", oid(tree)?],
        None,
    )?;
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("content tree record malformed")?;
        let header: Vec<_> = std::str::from_utf8(&record[..tab])?.split(' ').collect();
        if header.len() != 3 {
            return Err("content tree header malformed".into());
        }
        relative(std::str::from_utf8(&record[tab + 1..])?)?;
        if !matches!(
            (header[0], header[1]),
            ("040000", "tree") | ("100644", "blob") | ("100755", "blob")
        ) {
            return Err("content tree contains an unsupported mode/type".into());
        }
        add_object(root, &mut objects, header[2], header[1])?;
    }
    Ok(objects)
}
fn inventory(records: &[u8]) -> Result<BTreeMap<String, (FileMode, String)>> {
    let mut inventory = BTreeMap::new();
    for entry in entries(records)? {
        let mode = FileMode::parse(&entry.mode)?;
        if inventory.insert(entry.name, (mode, entry.object)).is_some() {
            return Err("duplicate logical content path".into());
        }
    }
    Ok(inventory)
}
fn file_hash(root: &Path, path: &str, mode: FileMode) -> Result<String> {
    let path = checked_file(root, path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let mut file = options.open(&path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err("content file handle is not regular".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (metadata.mode() & 0o111 != 0) != (mode == FileMode::Executable) {
            return Err("content file executable mode changed".into());
        }
    }
    let mut hash = blake3::Hasher::new();
    let mut bytes = [0; 8192];
    // Limit the read to the observed file size plus one, without buffering an
    // arbitrary file or claiming a fixed workload-size policy/Git wall timeout.
    let length = metadata.len();
    let mut limited = (&mut file).take(length.checked_add(1).ok_or("content file size overflow")?);
    let mut read = 0_u64;
    loop {
        let count = limited.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        read = read
            .checked_add(count as u64)
            .ok_or("content file read size overflow")?;
        hash.update(&bytes[..count]);
    }
    if read != length || file.metadata()?.len() != length {
        return Err("content file length changed during witness construction".into());
    }
    Ok(hash.finalize().to_hex().to_string())
}
fn object_records(objects: Objects) -> Result<Vec<ContentObject>> {
    // BTreeMap order is explicitly (full OID, lowercase kind). No insertion or
    // physical loose/packed-object representation order enters the subject.
    objects
        .into_iter()
        .map(|((oid, kind), raw_blake3)| {
            Ok(ContentObject {
                oid,
                kind: ObjectKind::parse(&kind)?,
                raw_blake3,
            })
        })
        .collect()
}
impl Snapshot {
    pub(crate) fn content_witness(&self) -> Result<ContentWitness> {
        if self.identity.kind == Kind::Working {
            return Err("content witness requires immutable Index or Commit inputs".into());
        }
        self.run_checked(|root| {
            let tree = self
                .identity
                .tree
                .as_ref()
                .ok_or("content witness tree missing")?;
            oid(tree)?;
            let private_inventory = inventory(&tree_records(root, tree)?)?;
            let private_objects = tree_objects(root, tree)?;
            let (kind, expected_inventory, source_blobs, objects) = match &self.identity.kind {
                Kind::Commit(commit) => {
                    oid(commit)?;
                    if resolve(&self.source, commit, "commit")? != *commit
                        || resolve(&self.source, commit, "tree")? != *tree
                        || resolve(root, commit, "tree")? != *tree
                    {
                        return Err("selected content commit/tree mismatch".into());
                    }
                    let expected_inventory = inventory(&tree_records(&self.source, commit)?)?;
                    let mut source = tree_objects(&self.source, tree)?;
                    add_object(&self.source, &mut source, commit, "commit")?;
                    let mut captured = self.objects.clone();
                    if self.probe.head != *commit {
                        captured.remove(&(self.probe.head.clone(), "commit".into()));
                    }
                    let mut private = private_objects;
                    add_object(root, &mut private, commit, "commit")?;
                    if source != captured || private != source {
                        return Err("selected source/private raw object content changed".into());
                    }
                    (
                        ContentKind::Commit {
                            commit: commit.clone(),
                            tree: tree.clone(),
                        },
                        expected_inventory,
                        source.clone(),
                        source,
                    )
                }
                Kind::Index => {
                    let expected_inventory = inventory(&self.probe.entries)?;
                    let mut source = BTreeMap::new();
                    add_object(&self.source, &mut source, &self.probe.head, "commit")?;
                    for (_, object) in expected_inventory.values() {
                        add_object(&self.source, &mut source, object, "blob")?;
                    }
                    for (key, hash) in &source {
                        if self.objects.get(key) != Some(hash) {
                            return Err("captured index source raw content changed".into());
                        }
                    }
                    for (_, object) in expected_inventory.values() {
                        let key = (object.clone(), "blob".into());
                        if private_objects.get(&key) != source.get(&key) {
                            return Err(
                                "private index blob content differs from captured source".into()
                            );
                        }
                    }
                    let mut objects = private_objects;
                    let key = (self.probe.head.clone(), "commit".into());
                    objects.insert(
                        key.clone(),
                        source
                            .get(&key)
                            .ok_or("source HEAD commit content absent")?
                            .clone(),
                    );
                    (
                        ContentKind::Index {
                            source_head: self.probe.head.clone(),
                            pending_tree: tree.clone(),
                        },
                        expected_inventory,
                        source,
                        objects,
                    )
                }
                Kind::Working => return Err("working content witness is unavailable".into()),
            };
            if private_inventory != expected_inventory {
                return Err(
                    "private path/mode/blob inventory differs from selected content".into(),
                );
            }
            let mut files = Vec::new();
            for (path, (mode, object)) in expected_inventory {
                let expected = source_blobs
                    .get(&(object.clone(), "blob".into()))
                    .ok_or("captured raw blob absent")?;
                let raw_blake3 = file_hash(root, &path, mode)?;
                if &raw_blake3 != expected {
                    return Err("private worktree content differs from captured raw blob".into());
                }
                files.push(ContentFile {
                    path,
                    mode,
                    blob_oid: object,
                    raw_blake3,
                });
            }
            let objects = object_records(objects)?;
            let payload = Payload {
                version: 1,
                algorithm: ALGORITHM,
                kind: &kind,
                files: &files,
                objects: &objects,
            };
            let mut digest = blake3::Hasher::new();
            digest.update(DOMAIN);
            digest.update(&serde_json::to_vec(&payload)?);
            Ok(ContentWitness {
                version: 1,
                algorithm: ALGORITHM.into(),
                kind,
                files,
                objects,
                digest: digest.finalize().to_hex().to_string(),
            })
        })
    }
}
