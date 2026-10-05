//! Local local evidence transport, NOT authenticated same-user evidence.
use super::{Capabilities, GateResult, Overall, Result};
use crate::gates::registry::{Cadence, Registry};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct EndpointProof {
    /// Scope-aware source HEAD/index/split index + exact source input bytes/modes.
    /// Working hashes worktree inputs; Index/Commit hash referenced Git object bytes.
    pub(crate) source: String,
    /// Includes private raw blob bytes/modes, private HEAD/index/tree.
    pub(crate) private: String,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct JudgeIdentity {
    pub(crate) executable_blake3: String,
    pub(crate) policy_revision: Option<String>,
    pub(crate) trust_closure_hash: Option<String>,
    pub(crate) mode: String, // "bootstrap-local" here; no prompt-role authentication.
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Metadata {
    /// Source candidate revision, NOT an index export's synthetic private HEAD.
    pub(crate) revision: String,
    pub(crate) base_revision: Option<String>,
    pub(crate) tree: Option<String>,
    pub(crate) scope: crate::gates::scopes::Kind,
    pub(crate) index_fingerprint: Option<String>,
    pub(crate) fingerprint: EndpointProof,
    pub(crate) judge: JudgeIdentity,
    /// Tags are keys only; values must be actually inspected immutable image IDs.
    /// None records unavailable inspection, never silently substitutes a tag.
    pub(crate) tool_image_actual_ids: BTreeMap<String, Option<String>>,
    pub(crate) capability_limits: Vec<String>,
    pub(crate) runtime_limits: Vec<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Ledger {
    pub(crate) schema: u32,
    pub(crate) authoritative: bool,
    pub(crate) metadata: Metadata,
    pub(crate) canonical_registry_hash: String,
    pub(crate) cadence: Cadence,
    pub(crate) suites: BTreeSet<String>,
    pub(crate) jobs: BTreeMap<String, bool>,
    pub(crate) capabilities: Capabilities,
    pub(crate) budgets: super::Budgets,
    pub(crate) results: Vec<GateResult>,
    pub(crate) overall: Overall,
    pub(crate) invalid_reasons: Vec<String>,
    pub(crate) duration_ms: u64,
    pub(crate) endpoints: Vec<super::triage::EndpointObservation>,
}

/// Algorithm v1: validated registry, stable struct field order, set-like arrays
/// sorted, jobs BTreeMap sorted, compact serde JSON, domain-separated BLAKE3.
/// Not RFC8785/JCS. Store the algorithm version alongside the hash prefix.
pub(crate) fn canonical_registry_hash(registry: &Registry) -> Result<String> {
    registry.fingerprint()
}

/// Caller supplies source and mount roots that evidence must not overlap,
/// including external caches and linked-worktree Git metadata. Canonical path
/// checks are hygiene, not same-UID security; a trusted judge must own policy.
pub(crate) struct PrivateOutput {
    directory: PathBuf,
}
impl PrivateOutput {
    pub(crate) fn new(directory: &Path, gate_writable_mounts: &[PathBuf]) -> Result<Self> {
        let directory = fs::canonicalize(directory)?;
        if !directory.is_dir() {
            return Err("private output must be an existing directory".into());
        }
        if gate_writable_mounts.is_empty() {
            return Err("declare gate-writable mounts explicitly".into());
        }
        for mount in gate_writable_mounts {
            let mount = fs::canonicalize(mount)?;
            if directory.starts_with(&mount) || mount.starts_with(&directory) {
                return Err("private evidence directory overlaps a gate-writable mount".into());
            }
        }
        Ok(Self { directory })
    }
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
    /// Reject aliases into reserved output and dangling symlinks before any
    /// output publication can create a target that the input path names.
    pub(crate) fn reject_input_alias(&self, input: &Path, label: &str) -> Result<()> {
        let canonical = fs::canonicalize(input).ok();
        let canonical_parent = input
            .parent()
            .and_then(|parent| fs::canonicalize(parent).ok());
        if input.starts_with(&self.directory)
            || canonical
                .as_ref()
                .is_some_and(|path| path.starts_with(&self.directory))
            || canonical_parent
                .as_ref()
                .is_some_and(|path| path.starts_with(&self.directory))
        {
            return Err(format!("{label} must not overlap evidence output").into());
        }
        let dangling_symlink = canonical.is_none()
            && fs::symlink_metadata(input).is_ok_and(|metadata| metadata.file_type().is_symlink());
        if dangling_symlink {
            return Err(format!(
                "{label} is an unresolved symlink; evidence output was left untouched"
            )
            .into());
        }
        Ok(())
    }
    pub(crate) fn ledger(&self, ledger: &Ledger) -> Result<PathBuf> {
        if ledger.authoritative {
            return Err("draft writer cannot assert trusted authority".into());
        }
        self.atomic("ledger.json", &serde_json::to_vec_pretty(ledger)?)
    }
    /// Use for process stdout+stderr on every exit, including zero exit.
    /// Filenames cannot escape output directory; input never names arbitrary paths.
    pub(crate) fn atomic(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
            || matches!(name, "." | "..")
        {
            return Err("invalid private output name".into());
        }
        #[cfg(not(unix))]
        {
            let _ = bytes;
            return Err("mode0600 output draft requires Unix".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Same filesystem -> atomic rename; restrictive mode BEFORE writing.
            let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
            temporary.write_all(bytes)?;
            temporary.as_file().sync_all()?;
            let target = self.directory.join(name);
            temporary.persist(&target).map_err(|error| error.error)?;
            fs::File::open(&self.directory)?.sync_all()?;
            Ok(target)
        }
    }
}
