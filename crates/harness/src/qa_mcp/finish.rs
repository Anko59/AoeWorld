//! Local claim publication with measured evidence; never an authenticated QA verdict.
use super::{Report, Value, json, qa};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn finish(report: &Report, evidence_dir: &Path) -> Result<Value, String> {
    qa::validate(report)?;
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("QA report exceeds 4 MiB".into());
    }
    publish(report, evidence_dir, &bytes)
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    let input = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    let mut output = PathBuf::new();
    for part in input.components() {
        match part {
            Component::RootDir => output.push(part.as_os_str()),
            Component::Normal(name) => output.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() {
                    return Err("invalid QA path".into());
                }
            }
            Component::Prefix(_) => return Err("unsupported QA path prefix".into()),
        }
    }
    Ok(output)
}

#[cfg(unix)]
mod local {
    use super::*;
    use nix::libc::{O_DIRECTORY, O_NOFOLLOW, O_NONBLOCK};
    use std::{
        fs::{File, Metadata, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
        os::unix::fs::{MetadataExt, OpenOptionsExt},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn identity(metadata: &Metadata) -> (u64, u64, u32, u32, u32) {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.mode(),
            metadata.uid(),
            metadata.gid(),
        )
    }
    fn leaf_identity(
        metadata: &Metadata,
    ) -> (u64, u64, u32, u32, u32, u64, u64, i64, i64, i64, i64) {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.mode(),
            metadata.uid(),
            metadata.gid(),
            metadata.nlink(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    }
    fn regular(path: &Path) -> Result<Option<Metadata>, String> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && metadata.nlink() == 1 => Ok(Some(metadata)),
            Ok(_) => Err("QA session leaf must be a normal singly-linked file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    struct Directory {
        root: PathBuf,
        ancestors: Vec<(PathBuf, File, Metadata)>,
    }
    impl Directory {
        fn open(path: &Path) -> Result<Self, String> {
            let root = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|error| error.to_string())?
                    .join(path)
            };
            let mut current = PathBuf::new();
            let mut ancestors = Vec::new();
            for component in root.components() {
                if !matches!(component, Component::RootDir | Component::Normal(_)) {
                    return Err("QA evidence directory requires normal absolute ancestors".into());
                }
                current.push(component.as_os_str());
                let metadata = fs::symlink_metadata(&current).map_err(|error| error.to_string())?;
                if !metadata.is_dir() {
                    return Err(
                        "QA evidence directory has a non-directory or symlink ancestor".into(),
                    );
                }
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(O_DIRECTORY | O_NOFOLLOW | O_NONBLOCK)
                    .open(&current)
                    .map_err(|error| error.to_string())?;
                if identity(&file.metadata().map_err(|error| error.to_string())?)
                    != identity(&metadata)
                {
                    return Err("QA evidence directory changed while opening".into());
                }
                ancestors.push((current.clone(), file, metadata));
            }
            if !root.is_absolute() || ancestors.is_empty() {
                return Err("invalid QA evidence root".into());
            }
            let result = Self { root, ancestors };
            result.check()?;
            Ok(result)
        }
        fn check(&self) -> Result<(), String> {
            for (path, file, original) in &self.ancestors {
                let current = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
                let held = file.metadata().map_err(|error| error.to_string())?;
                if !current.is_dir()
                    || identity(&current) != identity(original)
                    || identity(&held) != identity(original)
                {
                    return Err("QA evidence directory endpoint changed".into());
                }
            }
            Ok(())
        }
        fn sync(&self) -> Result<(), String> {
            self.ancestors
                .last()
                .ok_or("missing QA directory")?
                .1
                .sync_all()
                .map_err(|error| error.to_string())
        }
    }
    fn correlate(file: &mut File, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let held = file.metadata().map_err(|error| error.to_string())?;
        let current = regular(path)?.ok_or("QA report disappeared")?;
        if leaf_identity(&held) != leaf_identity(&current)
            || held.len() != bytes.len() as u64
            || held.mode() & 0o7777 != 0o600
        {
            return Err("QA pending/final report identity changed".into());
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        let mut observed = Vec::new();
        Read::by_ref(file)
            .take(bytes.len() as u64 + 1)
            .read_to_end(&mut observed)
            .map_err(|error| error.to_string())?;
        if observed != bytes
            || leaf_identity(&file.metadata().map_err(|error| error.to_string())?)
                != leaf_identity(&held)
            || leaf_identity(&regular(path)?.ok_or("QA report disappeared")?)
                != leaf_identity(&held)
        {
            return Err("QA report bytes changed during publication".into());
        }
        Ok(())
    }
    fn unchanged(path: &Path, original: &Option<Metadata>) -> Result<(), String> {
        let current = regular(path)?;
        if current.as_ref().map(leaf_identity) != original.as_ref().map(leaf_identity) {
            return Err("QA session changed before replacement".into());
        }
        Ok(())
    }
    fn references(report: &Report, session: &Path, old: &Option<Metadata>) -> Result<(), String> {
        for path in report
            .journeys
            .iter()
            .flat_map(|entry| &entry.evidence)
            .chain(report.findings.iter().flat_map(|entry| &entry.evidence))
        {
            let path = absolute(Path::new(path))?;
            if path == session || fs::canonicalize(&path).ok().as_deref() == Some(session) {
                return Err("QA session cannot be its own evidence".into());
            }
            if let (Some(old), Ok(metadata)) = (old, fs::metadata(&path))
                && (old.dev(), old.ino()) == (metadata.dev(), metadata.ino())
            {
                return Err("QA session evidence aliases the report".into());
            }
        }
        Ok(())
    }
    pub(super) fn publish(
        report: &Report,
        evidence_dir: &Path,
        bytes: &[u8],
    ) -> Result<Value, String> {
        let directory = Directory::open(evidence_dir)?;
        let session = directory.root.join("session.json");
        let original = regular(&session)?;
        references(report, &session, &original)?;
        directory.check()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let mut pending = None;
        for attempt in 0..16 {
            directory.check()?;
            let path = directory.root.join(format!(
                ".qa-report-pending-{}-{now}-{attempt}.json",
                std::process::id()
            ));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(O_NOFOLLOW | O_NONBLOCK)
                .open(&path)
            {
                Ok(file) => {
                    pending = Some((path, file));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let (pending, mut file) = pending.ok_or("QA pending-file collision limit exceeded")?;
        directory.check()?;
        correlate(&mut file, &pending, &[])?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        correlate(&mut file, &pending, bytes)?;
        directory.check()?;
        let observed = qa::observation::observe_file_at(&pending, &directory.root)
            .map_err(|error| error.to_string())?;
        directory.check()?;
        correlate(&mut file, &pending, bytes)?;
        unchanged(&session, &original)?;
        // Both resolved absolute parents and intended fixed/generated leaf names
        // are verified before the only move. Endpoint correlation is not race immunity.
        if pending.parent() != Some(directory.root.as_path())
            || session.parent() != Some(directory.root.as_path())
            || session.file_name().and_then(|name| name.to_str()) != Some("session.json")
            || !pending
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(".qa-report-pending-") && name.ends_with(".json")
                })
        {
            return Err("QA publication targets are not intended owned paths".into());
        }
        fs::rename(&pending, &session).map_err(|error| error.to_string())?;
        directory.sync()?;
        directory.check()?;
        correlate(&mut file, &session, bytes)?;
        let final_observation = qa::observation::observe_file_at(&session, &directory.root)
            .map_err(|error| error.to_string())?;
        if serde_json::to_value(&observed).map_err(|error| error.to_string())?
            != serde_json::to_value(&final_observation).map_err(|error| error.to_string())?
        {
            return Err("QA evidence changed during report publication".into());
        }
        directory.check()?;
        correlate(&mut file, &session, bytes)?;
        Ok(
            json!({"report":session,"status":report.status,"claimed_status":report.status,"assessment":"STRUCTURAL_EVIDENCE_OBSERVED_NON_AUTHORITATIVE","authoritative":false,"observation":final_observation}),
        )
    }
}
#[cfg(unix)]
use local::publish;
#[cfg(not(unix))]
fn publish(_: &Report, _: &Path, _: &[u8]) -> Result<Value, String> {
    Err("QA report publication requires Unix no-follow file observations".into())
}

#[cfg(test)]
mod tests;
