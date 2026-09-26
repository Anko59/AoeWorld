use serde::Serialize;
use std::{error::Error, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const CORPUS_FILE_LIMIT: u64 = 16_384;
const CORPUS_BYTE_LIMIT: u64 = 64 * 1024 * 1024;
const ARTIFACT_FILE_LIMIT: u64 = 256;
const ARTIFACT_BYTE_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Policy {
    pub corpus_file_limit: u64,
    pub corpus_byte_limit: u64,
    pub artifact_file_limit: u64,
    pub artifact_byte_limit: u64,
    pub retention: &'static str,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            corpus_file_limit: CORPUS_FILE_LIMIT,
            corpus_byte_limit: CORPUS_BYTE_LIMIT,
            artifact_file_limit: ARTIFACT_FILE_LIMIT,
            artifact_byte_limit: ARTIFACT_BYTE_LIMIT,
            retention: "archive before coverage-guided active minimization; retain seeds and crashes",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Usage {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Snapshot {
    pub corpus: Usage,
    pub artifacts: Usage,
}

impl Policy {
    pub(super) fn inspect(&self, root: &Path) -> Result<Snapshot> {
        Ok(Snapshot {
            corpus: usage(&root.join("fuzz/corpus"))?,
            artifacts: usage(&root.join("fuzz/artifacts"))?,
        })
    }

    pub(super) fn validate(&self, snapshot: Snapshot) -> Result<()> {
        for (name, actual, limit) in [
            (
                "corpus files",
                snapshot.corpus.files,
                self.corpus_file_limit,
            ),
            (
                "corpus bytes",
                snapshot.corpus.bytes,
                self.corpus_byte_limit,
            ),
            (
                "artifact files",
                snapshot.artifacts.files,
                self.artifact_file_limit,
            ),
            (
                "artifact bytes",
                snapshot.artifacts.bytes,
                self.artifact_byte_limit,
            ),
        ] {
            if actual > limit {
                return Err(format!("fuzz {name} quota exceeded: {actual} > {limit}").into());
            }
        }
        Ok(())
    }
}

pub(super) fn near_limit(policy: Policy, snapshot: Snapshot) -> bool {
    [
        (snapshot.corpus.files, policy.corpus_file_limit),
        (snapshot.corpus.bytes, policy.corpus_byte_limit),
        (snapshot.artifacts.files, policy.artifact_file_limit),
        (snapshot.artifacts.bytes, policy.artifact_byte_limit),
    ]
    .into_iter()
    .any(|(actual, limit)| actual >= limit.saturating_sub(limit / 20))
}

pub(super) fn usage(directory: &Path) -> Result<Usage> {
    let mut usage = Usage { files: 0, bytes: 0 };
    collect(directory, &mut usage)?;
    Ok(usage)
}

fn collect(directory: &Path, usage: &mut Usage) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        collect_entry(entry, usage)?;
    }
    Ok(())
}

fn collect_entry(entry: fs::DirEntry, usage: &mut Usage) -> Result<()> {
    let kind = match entry.file_type() {
        Ok(kind) => kind,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let path = entry.path();
    if kind.is_dir() {
        collect(&path, usage)
    } else if kind.is_file() {
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        usage.files = usage
            .files
            .checked_add(1)
            .ok_or("fuzz file count overflow")?;
        usage.bytes = usage
            .bytes
            .checked_add(metadata.len())
            .ok_or("fuzz byte count overflow")?;
        Ok(())
    } else {
        Err(format!(
            "fuzz storage contains a non-regular path: {}",
            path.display()
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_quotas_fail_without_deleting_or_changing_preserved_files() {
        let root = tempfile::tempdir().expect("directory");
        let corpus = root.path().join("fuzz/corpus/drs");
        let artifacts = root.path().join("fuzz/artifacts/map_chunk");
        fs::create_dir_all(&corpus).expect("corpus directory");
        fs::create_dir_all(&artifacts).expect("artifact directory");
        fs::write(corpus.join("preserved-seed"), b"corpus bytes").expect("corpus file");
        fs::write(artifacts.join("preserved-crash"), b"artifact bytes").expect("artifact file");

        let policy = Policy::default();
        let snapshot = policy.inspect(root.path()).expect("bounded storage");
        policy.validate(snapshot).expect("bounded storage");
        assert_eq!(
            snapshot.corpus,
            Usage {
                files: 1,
                bytes: 12
            }
        );
        assert_eq!(
            snapshot.artifacts,
            Usage {
                files: 1,
                bytes: 14
            }
        );

        let strict = Policy {
            corpus_file_limit: 0,
            corpus_byte_limit: 0,
            artifact_file_limit: 0,
            artifact_byte_limit: 0,
            retention: Policy::default().retention,
        };
        let before_growth = Snapshot {
            corpus: Usage { files: 0, bytes: 0 },
            artifacts: Usage { files: 0, bytes: 0 },
        };
        assert!(strict.validate(before_growth).is_ok());
        assert!(strict.validate(snapshot).is_err());
        assert!(
            strict
                .validate(strict.inspect(root.path()).unwrap())
                .is_err()
        );
        assert_eq!(
            fs::read(corpus.join("preserved-seed")).unwrap(),
            b"corpus bytes"
        );
        assert_eq!(
            fs::read(artifacts.join("preserved-crash")).unwrap(),
            b"artifact bytes"
        );
        assert_eq!(policy.inspect(root.path()).unwrap(), snapshot);
    }

    #[test]
    fn storage_policy_rejects_non_regular_files() {
        let root = tempfile::tempdir().expect("directory");
        let corpus = root.path().join("fuzz/corpus");
        fs::create_dir_all(&corpus).expect("corpus directory");
        std::os::unix::fs::symlink(root.path(), corpus.join("outside")).expect("symlink");
        assert!(Policy::default().inspect(root.path()).is_err());
    }

    #[test]
    fn working_storage_guard_trips_before_the_fixed_quota() {
        let policy = Policy::default();
        let mut snapshot = Snapshot {
            corpus: Usage {
                files: 15_000,
                bytes: 0,
            },
            artifacts: Usage { files: 0, bytes: 0 },
        };
        assert!(!near_limit(policy, snapshot));
        snapshot.corpus.files = 15_565;
        assert!(near_limit(policy, snapshot));
        assert!(policy.validate(snapshot).is_ok());
    }

    #[test]
    fn disappearing_corpus_entry_is_transient_during_live_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("temporary");
        fs::write(&path, b"input").unwrap();
        let entry = fs::read_dir(root.path()).unwrap().next().unwrap().unwrap();
        fs::remove_file(path).unwrap();
        let mut usage = Usage { files: 0, bytes: 0 };
        collect_entry(entry, &mut usage).unwrap();
        assert_eq!(usage, Usage { files: 0, bytes: 0 });
    }
}
