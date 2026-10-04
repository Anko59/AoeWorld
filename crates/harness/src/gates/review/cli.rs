use super::*;
use crate::{
    gates::runner::{evidence::PrivateOutput, signals::Signals},
    process::Cancellation,
};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
#[derive(clap::Args)]
pub(crate) struct Options {
    /// Required full immutable base commit ID, never a movable ref.
    #[arg(long)]
    pub(super) base: String,
    /// Full immutable candidate commit ID, mutually exclusive with captured index.
    #[arg(long, conflicts_with = "index", required_unless_present = "index")]
    pub(super) candidate: Option<String>,
    /// Capture intentional effective Git index exactly once; not a candidate commit.
    #[arg(long, conflicts_with = "candidate")]
    pub(super) index: bool,
    /// Absolute existing output directory outside checkout/common Git metadata.
    #[arg(long)]
    pub(super) output: PathBuf,
    /// Absolute bounded duplicate-free review JSON; byte agreement is not authority.
    #[arg(long)]
    pub(super) review: Option<PathBuf>,
}
fn full_oid(oid: &str) -> Result<()> {
    if !matches!(oid.len(), 40 | 64)
        || !oid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("review revisions must be full lowercase immutable Git object IDs".into());
    }
    Ok(())
}
fn input(path: &Path) -> Result<Vec<u8>> {
    if !path.is_absolute() {
        return Err("review input must be absolute".into());
    }
    let mut actual = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::CurDir | Component::ParentDir) {
            return Err("review input must be normal".into());
        }
        actual.push(part);
        if fs::symlink_metadata(&actual)?.file_type().is_symlink() {
            return Err("review input contains symlink".into());
        }
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let mut file = options.open(actual)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("review input is not bounded regular JSON".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("review input has hardlink alias".into());
        }
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES
        || bytes.len() as u64 != metadata.len()
        || file.metadata()?.len() != metadata.len()
    {
        return Err("review input changed length while reading".into());
    }
    Ok(bytes)
}
pub(crate) fn execute(root: &Path, options: Options) -> Result<()> {
    let root = fs::canonicalize(root)?;
    if !options.output.is_absolute() {
        return Err("review output must be absolute".into());
    }
    let output = PrivateOutput::new(
        &options.output,
        &[root.clone(), crate::hooks::common_directory(&root)?],
    )?;
    if let Some(review) = &options.review {
        output.reject_input_alias(review, "review input")?;
    }
    // Alias rejection precedes every write, including pending invalidation.
    output.atomic("observation.json",b"{\"schema\":1,\"status\":\"UNAVAILABLE\",\"authoritative\":false,\"authenticated_verdict\":\"UNAVAILABLE\"}")?;
    let cancellation = Cancellation::default();
    let signals = Signals::new(&cancellation);
    let result = (|| -> Result<()> {
        signals.as_ref().map_err(|error| error.to_string())?;
        full_oid(&options.base)?;
        let candidate = match (&options.candidate, options.index) {
            (Some(oid), false) => {
                full_oid(oid)?;
                Snapshot::prepare_independent(&root, Kind::Commit(oid.clone()))?
            }
            (None, true) => Snapshot::prepare(&root, Kind::Index)?,
            _ => return Err("provide exactly one full candidate commit or --index".into()),
        };
        let base = Snapshot::prepare_independent(&root, Kind::Commit(options.base.clone()))?;
        let review_bytes = options.review.as_deref().map(input).transpose()?;
        let review = review_bytes
            .as_deref()
            .map(schema::Review::parse)
            .transpose()?;
        if cancellation.cancelled() {
            return Err("review subject materialization cancelled".into());
        }
        let subject = materialize(&base, &candidate)?;
        let mut observation = assessment(&subject, review.as_ref())?;
        observation["base_observation"] =
            json!({"identity":base.identity,"fingerprints":base.fingerprints()?});
        observation["candidate_observation"] =
            json!({"identity":candidate.identity,"fingerprints":candidate.fingerprints()?});
        observation["review_bytes_blake3"] = review_bytes
            .as_ref()
            .map(|bytes| json!(blake3::hash(bytes).to_hex().to_string()))
            .unwrap_or(Value::Null);
        recheck(&base, &candidate, &subject)?;
        if let Some(path) = &options.review
            && Some(input(path)?) != review_bytes
        {
            return Err("review input changed before publication".into());
        }
        if cancellation.cancelled() {
            return Err("review subject materialization cancelled".into());
        }
        // Only the observation marks acceptance. Until both endpoint checks finish,
        // old or newly written subject bytes remain pending/non-authoritative.
        output.atomic("subject.json", &encoded(&subject)?)?;
        recheck(&base, &candidate, &subject)?;
        if let Some(path) = &options.review
            && Some(input(path)?) != review_bytes
        {
            return Err("review input changed during publication".into());
        }
        if cancellation.cancelled() {
            return Err("review subject materialization cancelled".into());
        }
        output.atomic(
            "observation.json",
            &serde_json::to_vec_pretty(&observation)?,
        )?;
        println!("{}", serde_json::to_string_pretty(&observation)?);
        Ok(())
    })();
    if result.is_err() {
        output.atomic("observation.json",b"{\"schema\":1,\"status\":\"UNAVAILABLE\",\"authoritative\":false,\"authenticated_verdict\":\"UNAVAILABLE\",\"reason\":\"materialization or exact comparison failed\"}")?;
    }
    result
}
