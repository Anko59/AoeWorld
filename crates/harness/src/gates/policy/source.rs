use super::*;

/// This configuration is supplied by the launcher OUTSIDE candidate-controlled
/// mounts. Reading it alone does not authenticate the launcher or grant authority.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Anchor {
    pub schema: u32,
    pub repository: String,
    pub repository_id: u64,
    pub remote_url: String,
    pub integration_branch: String,
}
impl Anchor {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let anchor: Self = serde_json::from_slice(bytes)?;
        anchor.validate()?;
        Ok(anchor)
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1 || self.repository_id == 0 || self.integration_branch != "dev" {
            return Err("unsupported source anchor; only protected dev policy is supported".into());
        }
        let parts: Vec<_> = self.repository.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || matches!(*part, "." | "..")
                    || !part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
            })
        {
            return Err("invalid configured repository identity".into());
        }
        if self.remote_url != format!("https://github.com/{}.git", self.repository) {
            return Err("remote must be exact credential-free configured GitHub HTTPS URL".into());
        }
        Ok(())
    }
}

/// Normalized from a trusted backend's authenticated GitHub API responses, not
/// JSON supplied by the candidate. Keep construction inside the backend module.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ObservedBranch {
    pub repository: String,
    pub repository_id: u64,
    pub branch: String,
    pub protected: bool,
    pub commit: String,
    pub required_contexts: BTreeSet<String>,
    pub strict: bool,
    pub observed_at_unix_s: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceIdentity {
    pub repository: String,
    pub repository_id: u64,
    pub remote_url: String,
    pub protected_ref: String,
    pub commit: String,
    pub tree: String,
    pub observed_at_unix_s: u64,
}

impl ObservedBranch {
    /// Resolve exactly ONCE. API identity, protected branch contract, fresh
    /// remote ls-remote full OID must agree. A moving branch is retry/UNAVAILABLE,
    /// never silently re-resolved midway through a bundle or run.
    pub(crate) fn resolve(
        &self,
        anchor: &Anchor,
        advertised_commit: &str,
        tree: &str,
    ) -> Result<SourceIdentity> {
        anchor.validate()?;
        full_oid(&self.commit)?;
        full_oid(advertised_commit)?;
        full_oid(tree)?;
        if self.repository != anchor.repository
            || self.repository_id != anchor.repository_id
            || self.branch != anchor.integration_branch
        {
            return Err(
                "remote repository/branch identity does not match configured launcher anchor"
                    .into(),
            );
        }
        if !self.protected || !self.strict || !self.required_contexts.contains("required") {
            return Err("integration policy branch protection was not observed".into());
        }
        if self.commit != advertised_commit {
            return Err(
                "protected base moved during resolution; retry without adopting another policy"
                    .into(),
            );
        }
        Ok(SourceIdentity {
            repository: anchor.repository.clone(),
            repository_id: anchor.repository_id,
            remote_url: anchor.remote_url.clone(),
            protected_ref: "refs/heads/dev".into(),
            commit: self.commit.clone(),
            tree: tree.to_owned(),
            observed_at_unix_s: self.observed_at_unix_s,
        })
    }
}

// Actual observations originate in backend.rs, never candidate JSON or refs.
