//! Preserve discovered inputs before coverage-guided active-corpus reduction.
use super::{Result, TARGETS, seeds, storage};
use serde::Serialize;
use std::{fs, io::ErrorKind, path::Path, time::Duration};

const TARGET_FILE_TRIGGER: u64 = 1_024;
const TOTAL_FILE_TRIGGER: u64 = 8_192;

#[derive(Debug, Serialize)]
pub(super) struct Action {
    pub target: &'static str,
    pub archived_files: u64,
    pub active_before: storage::Usage,
    pub active_after: storage::Usage,
}

pub(super) fn run<F>(root: &Path, policy: storage::Policy, mut minimize: F) -> Result<Vec<Action>>
where
    F: FnMut(&[&str], Duration) -> Result<()>,
{
    let total = policy.inspect(root)?.corpus.files;
    let mut actions = Vec::new();
    for target in TARGETS {
        let directory = root.join("fuzz/corpus").join(target);
        let before = storage::usage(&directory)?;
        if before.files <= TARGET_FILE_TRIGGER
            && total <= TOTAL_FILE_TRIGGER
            && total <= policy.corpus_file_limit
        {
            continue;
        }
        let archived_files = archive(&directory, &root.join("fuzz/archive").join(target))?;
        minimize(
            &[
                "+nightly-2026-09-01",
                "fuzz",
                "cmin",
                target,
                "--",
                "-max_len=1048576",
                "-timeout=5",
            ],
            Duration::from_secs(1_800),
        )
        .map_err(|error| {
            format!(
                "target {target}: archived {archived_files} inputs at fuzz/archive/{target}; coverage minimization failed: {error}"
            )
        })?;
        // cmin is allowed to remove redundant active inputs. Restore every
        // canonical seed after it finishes; archived inputs remain immutable.
        seeds::prepare(root)?;
        let after = storage::usage(&directory)?;
        actions.push(Action {
            target,
            archived_files,
            active_before: before,
            active_after: after,
        });
    }
    policy.validate(policy.inspect(root)?)?;
    Ok(actions)
}

fn archive(source: &Path, destination: &Path) -> Result<u64> {
    fs::create_dir_all(destination)?;
    let mut count = 0;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(format!("non-regular corpus input: {}", entry.path().display()).into());
        }
        let bytes = fs::read(entry.path())?;
        let path = destination.join(blake3::hash(&bytes).to_hex().to_string());
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(&bytes)?;
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if fs::read(&path)? != bytes {
                    return Err("fuzz archive digest collision".into());
                }
            }
            Err(error) => return Err(error.into()),
        }
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_inputs_before_failed_minimization() {
        let root = tempfile::tempdir().unwrap();
        let corpus = root.path().join("fuzz/corpus/drs");
        fs::create_dir_all(&corpus).unwrap();
        fs::write(corpus.join("discovered"), b"discovered bytes").unwrap();
        let policy = storage::Policy {
            corpus_file_limit: 0,
            ..storage::Policy::default()
        };
        let failure = run(root.path(), policy, |args, _| {
            assert_eq!(args[2], "cmin");
            Err("injected maintenance failure".into())
        })
        .unwrap_err();
        assert!(failure.to_string().contains("injected maintenance failure"));
        assert_eq!(
            fs::read(corpus.join("discovered")).unwrap(),
            b"discovered bytes"
        );
        let digest = blake3::hash(b"discovered bytes").to_hex().to_string();
        assert_eq!(
            fs::read(root.path().join("fuzz/archive/drs").join(digest)).unwrap(),
            b"discovered bytes"
        );
    }

    #[test]
    fn archive_rejects_nonregular_inputs() {
        let root = tempfile::tempdir().unwrap();
        let corpus = root.path().join("corpus");
        fs::create_dir_all(&corpus).unwrap();
        std::os::unix::fs::symlink(root.path(), corpus.join("link")).unwrap();
        assert!(archive(&corpus, &root.path().join("archive")).is_err());
    }
}
