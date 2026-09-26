//! Preserve discovered inputs before coverage-guided active-corpus reduction.
use super::{Result, TARGETS, seeds, storage};
use serde::Serialize;
use std::{fs, io::ErrorKind, path::Path, time::Duration};

const TARGET_FILE_TRIGGER: u64 = 1_024;
const TOTAL_FILE_TRIGGER: u64 = 8_192;

#[derive(Clone, Debug, Serialize)]
pub(super) struct Action {
    pub target: &'static str,
    pub after_target: Option<&'static str>,
    pub after_segment: Option<u8>,
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
        actions.push(minimize_target(root, target, None, before, &mut minimize)?);
    }
    policy.validate(policy.inspect(root)?)?;
    Ok(actions)
}

/// Completed targets can grow the active corpus before the next 300-second
/// campaign starts. Preserve and minimize that target when growth is material;
/// then recover headroom from other large corpora if necessary. Never alter a
/// corpus concurrently with libFuzzer.
pub(super) fn after_target<F>(
    root: &Path,
    policy: storage::Policy,
    target: &'static str,
    before: storage::Snapshot,
    after: storage::Snapshot,
    mut minimize: F,
) -> Result<Vec<Action>>
where
    F: FnMut(&[&str], Duration) -> Result<()>,
{
    const GROWTH_TRIGGER: u64 = 512;
    const HEADROOM_PERCENT: u64 = 85;
    let needs_headroom = |snapshot: storage::Snapshot| {
        snapshot.corpus.files.saturating_mul(100)
            >= policy.corpus_file_limit.saturating_mul(HEADROOM_PERCENT)
            || snapshot.corpus.bytes.saturating_mul(100)
                >= policy.corpus_byte_limit.saturating_mul(HEADROOM_PERCENT)
    };
    let mut actions = Vec::new();
    if after.corpus.files.saturating_sub(before.corpus.files) >= GROWTH_TRIGGER
        || needs_headroom(after)
    {
        let directory = root.join("fuzz/corpus").join(target);
        let usage = storage::usage(&directory)?;
        actions.push(minimize_target(
            root,
            target,
            Some(target),
            usage,
            &mut minimize,
        )?);
    }
    let mut current = policy.inspect(root)?;
    if needs_headroom(current) {
        let mut candidates = TARGETS
            .iter()
            .copied()
            .filter(|candidate| *candidate != target)
            .map(|candidate| {
                Ok((
                    candidate,
                    storage::usage(&root.join("fuzz/corpus").join(candidate))?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        candidates.sort_by_key(|(_, usage)| std::cmp::Reverse(usage.files));
        for (candidate, usage) in candidates {
            if !needs_headroom(current) {
                break;
            }
            if usage.files <= TARGET_FILE_TRIGGER {
                continue;
            }
            actions.push(minimize_target(
                root,
                candidate,
                Some(target),
                usage,
                &mut minimize,
            )?);
            current = policy.inspect(root)?;
        }
    }
    policy.validate(current)?;
    if needs_headroom(current) {
        return Err(format!(
            "after target {target}: coverage-preserving minimization could not restore 15% corpus headroom: {current:?}"
        )
        .into());
    }
    Ok(actions)
}

fn minimize_target<F>(
    root: &Path,
    target: &'static str,
    after_target: Option<&'static str>,
    before: storage::Usage,
    minimize: &mut F,
) -> Result<Action>
where
    F: FnMut(&[&str], Duration) -> Result<()>,
{
    let directory = root.join("fuzz/corpus").join(target);
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
    Ok(Action {
        target,
        after_target,
        after_segment: None,
        archived_files,
        active_before: before,
        active_after: after,
    })
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

    #[test]
    fn completed_target_is_archived_and_minimized_before_next_campaign() {
        let root = tempfile::tempdir().unwrap();
        let corpus = root.path().join("fuzz/corpus/manifest");
        fs::create_dir_all(&corpus).unwrap();
        for index in 0..88 {
            fs::write(corpus.join(format!("input-{index}")), index.to_string()).unwrap();
        }
        let policy = storage::Policy {
            corpus_file_limit: 100,
            ..storage::Policy::default()
        };
        let before = storage::Snapshot {
            corpus: storage::Usage {
                files: 50,
                bytes: 0,
            },
            artifacts: storage::Usage { files: 0, bytes: 0 },
        };
        let after = policy.inspect(root.path()).unwrap();
        let actions = after_target(root.path(), policy, "manifest", before, after, |args, _| {
            assert_eq!(args[2], "cmin");
            assert_eq!(args[3], "manifest");
            for index in 0..48 {
                fs::remove_file(corpus.join(format!("input-{index}"))).unwrap();
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].after_target, Some("manifest"));
        assert_eq!(actions[0].archived_files, 88);
        assert!(root.path().join("fuzz/archive/manifest").is_dir());
        assert!(policy.inspect(root.path()).unwrap().corpus.files < 85);
    }

    #[test]
    fn small_completed_target_does_not_repeat_minimization() {
        let root = tempfile::tempdir().unwrap();
        let policy = storage::Policy::default();
        let snapshot = policy.inspect(root.path()).unwrap();
        let actions = after_target(root.path(), policy, "drs", snapshot, snapshot, |_, _| {
            panic!("maintenance was not needed")
        })
        .unwrap();
        assert!(actions.is_empty());
    }
}
