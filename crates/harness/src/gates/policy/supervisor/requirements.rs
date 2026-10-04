use super::*;
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Requirements {
    schema: u32,
    anchor: Anchor,
    service_uid: u32,
    candidate_uid: u32,
    artifact_root: PathBuf,
    evidence_root: PathBuf,
    lease_root: PathBuf,
    subject: Subject,
    resources: Resources,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    schema: u32,
    repository_id: u64,
    protected_ref: String,
    protected_commit: String,
    protected_tree: String,
    closure_blake3: String,
    registry_hash: String,
    executable_blake3: String,
    runtime_blake3: String,
    trust_root_id: String,
    abi: Abi,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Resources {
    memory_mib: u32,
    pids: u32,
    cpus: u32,
    workload_s: u32,
    cleanup_s: u32,
    command_s: u32,
}
impl Requirements {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 32768 {
            return Err("supervisor requirements exceed 32KiB".into());
        }
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1 || self.subject.schema != 1 {
            return Err("unsupported supervisor requirements/subject schema".into());
        }
        Anchor::parse(&serde_json::to_vec(&self.anchor)?)?;
        if self.service_uid == self.candidate_uid || self.candidate_uid == 0 {
            return Err("candidate UID must be nonroot and distinct from service UID".into());
        }
        if self.subject.repository_id != self.anchor.repository_id
            || self.subject.protected_ref != "refs/heads/dev"
        {
            return Err("artifact subject differs from fixed protected repository/ref".into());
        }
        full_oid(&self.subject.protected_commit)?;
        full_oid(&self.subject.protected_tree)?;
        for hash in [
            &self.subject.closure_blake3,
            &self.subject.executable_blake3,
            &self.subject.runtime_blake3,
            &self.subject.trust_root_id,
        ] {
            digest(hash)?;
        }
        digest(
            self.subject
                .registry_hash
                .strip_prefix("blake3:registry-v2-canonical-v1:")
                .ok_or("subject registry fingerprint domain missing")?,
        )?;
        let abi = Abi::parse(&serde_json::to_vec(&self.subject.abi)?)?;
        let expected: BTreeSet<_> = [
            Operation::FmtCheck,
            Operation::StructureCheck,
            Operation::ArchitectureCheck,
            Operation::DocsCheck,
            Operation::Lint,
            Operation::TestUnit,
        ]
        .iter()
        .map(Operation::argument)
        .collect();
        if abi.images.is_empty()
            || abi.images.len() > 16
            || abi
                .dispatch
                .iter()
                .map(|item| item.operation.argument())
                .collect::<BTreeSet<_>>()
                != expected
        {
            return Err(
                "supervisor subject must bind all six closed operations and bounded image roles"
                    .into(),
            );
        }
        let limit = &self.resources;
        if !(64..=32768).contains(&limit.memory_mib)
            || !(1..=1024).contains(&limit.pids)
            || !(1..=32).contains(&limit.cpus)
            || !(1..=3600).contains(&limit.workload_s)
            || !(1..=15).contains(&limit.cleanup_s)
            || !(1..=5).contains(&limit.command_s)
            || limit.command_s > limit.cleanup_s
        {
            return Err("supervisor resource/cleanup bounds invalid".into());
        }
        let mut roots = Vec::new();
        for root in [&self.artifact_root, &self.evidence_root, &self.lease_root] {
            let path = plain_absolute(root)?;
            if !fs::metadata(&path)?.is_dir()
                || path == Path::new("/")
                || path.to_string_lossy().contains([':', ','])
            {
                return Err("supervisor root must be a bounded normal directory".into());
            }
            if roots
                .iter()
                .any(|other: &PathBuf| path.starts_with(other) || other.starts_with(&path))
            {
                return Err("supervisor roots overlap".into());
            }
            roots.push(path);
        }
        Ok(())
    }
    pub(super) fn report(&self, candidate_root: &Path) -> Result<Value> {
        let candidate_root = fs::canonicalize(candidate_root)?;
        let mut observations = Vec::new();
        for root in [&self.artifact_root, &self.evidence_root, &self.lease_root] {
            let path = plain_absolute(root)?;
            if path.starts_with(&candidate_root) || candidate_root.starts_with(&path) {
                return Err("supervisor root overlaps candidate checkout".into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let metadata = fs::metadata(&path)?;
                observations.push(json!({"path":path,"observed_uid":metadata.uid(),"observed_mode":metadata.mode() & 0o777,"matches_declared_service_uid":metadata.uid()==self.service_uid}));
            }
            #[cfg(not(unix))]
            observations.push(json!({"path":path,"ownership":"UNAVAILABLE"}));
        }
        let operations: Vec<_> = self.subject.abi.dispatch.iter().map(|item| json!({"operation":item.operation.argument(),"image":item.image,"entrypoint":["/judge/aoe-harness",item.operation.argument()]})).collect();
        Ok(
            json!({"schema":1,"status":"UNAVAILABLE","authoritative":false,"subject":self.subject,"declared_service_uid":self.service_uid,"declared_candidate_uid":self.candidate_uid,"root_observations":observations,
            "reasons":["subject fields are unverified assertions, not a signed artifact proof","approved external artifact verifier and trust root unavailable","externally authenticated isolated deployment unavailable; current coding host has daemon control","filesystem UID/mode and local hashes cannot authenticate supervisor authority"],
            "worker_template":{"execution":"NOT_IMPLEMENTED","uid":self.candidate_uid,"network":"none","cap_drop":["ALL"],"no_new_privileges":true,"read_only":true,"environment":{},"supplementary_groups":[],"docker_socket":false,"host_pid_ipc_network":false,"writable_judge_evidence":false,"shared_trusted_caches":false,"resources":self.resources,"operations":operations},
            "limits":["no signature verification, protected build/deployment admission or worker execution","root observations do not prove ancestor permissions, ACLs, alternate daemon endpoints or runtime isolation","trusted verdict computation and hidden tests must remain outside the hostile worker namespace","human protected ABI/image/artifact migration remains required"]}),
        )
    }
}
