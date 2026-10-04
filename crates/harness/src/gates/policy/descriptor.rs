use super::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImagePin {
    pub name: String,
    /// Protected manifest OCI manifest digest, never mutable tag.
    pub reference: String,
    /// Expected local config ID bound in the protected manifest/build attestation.
    pub actual_id: String,
}
impl ImagePin {
    fn validate(&self) -> Result<()> {
        if self.name.is_empty()
            || !self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        {
            return Err("invalid image role".into());
        }
        let (repository, hash) = self
            .reference
            .rsplit_once("@sha256:")
            .ok_or("image reference is not content pinned")?;
        if repository != format!("ghcr.io/anko59/aoeworld/{}", self.name) {
            return Err("unexpected protected image repository".into());
        }
        digest(hash)?;
        digest(
            self.actual_id
                .strip_prefix("sha256:")
                .ok_or("actual image config ID missing")?,
        )?;
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Operation {
    FmtCheck,
    StructureCheck,
    ArchitectureCheck,
    DocsCheck,
    Lint,
    TestUnit,
}
impl Operation {
    pub(crate) fn argument(&self) -> &'static str {
        match self {
            Self::FmtCheck => "fmt-check",
            Self::StructureCheck => "structure-check",
            Self::ArchitectureCheck => "architecture-check",
            Self::DocsCheck => "docs-check",
            Self::Lint => "lint",
            Self::TestUnit => "test-unit",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dispatch {
    pub gate: String,
    pub operation: Operation,
    pub image: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Abi {
    pub schema: u32,
    pub abi: u32,
    pub registry_schema: u32,
    pub images: Vec<ImagePin>,
    pub dispatch: Vec<Dispatch>,
}
impl Abi {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        let abi: Self = serde_json::from_slice(bytes)?;
        if (abi.schema, abi.abi, abi.registry_schema) != (1, 1, 2) {
            return Err("unsupported protected judge ABI".into());
        }
        let mut images = BTreeSet::new();
        for image in &abi.images {
            image.validate()?;
            if !images.insert(image.name.clone()) {
                return Err("duplicate image role".into());
            }
        }
        let mut commands = BTreeSet::new();
        for command in &abi.dispatch {
            // Typed operations exclude publication/merging/arbitrary shell.
            if command.gate != command.operation.argument()
                || !images.contains(&command.image)
                || !commands.insert(command.gate.clone())
            {
                return Err(
                    "dispatch is not a unique allowlisted validation command with pinned image"
                        .into(),
                );
            }
        }
        Ok(abi)
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Status {
    PreparedNonAuthoritative,
    Unavailable,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Preparation {
    pub schema: u32,
    pub authoritative: bool,
    pub status: Status,
    pub source: SourceIdentity,
    pub closure_blake3: String,
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub canonical_registry_hash: Option<String>,
    pub gates: Vec<String>,
    pub image_pins: Vec<ImagePin>,
    pub reasons: Vec<String>,
}
impl Preparation {
    pub(crate) fn prepare(
        policy: &Materialized,
        candidate_commit: &str,
        candidate_tree: &str,
        paths: &[String],
        cadence: Cadence,
        observed_images: &BTreeMap<String, String>,
    ) -> Result<Self> {
        policy.verify()?;
        full_oid(candidate_commit)?;
        full_oid(candidate_tree)?;
        let closure = policy.closure();
        let mut result = Self {
            schema: 1,
            authoritative: false,
            status: Status::Unavailable,
            source: closure.source.clone(),
            closure_blake3: closure.blake3.clone(),
            candidate_commit: candidate_commit.into(),
            candidate_tree: candidate_tree.into(),
            canonical_registry_hash: None,
            gates: vec![],
            image_pins: vec![],
            reasons: vec![],
        };
        // Check inventory, NOT path.exists (symlink would not count as absent).
        if !closure
            .entries
            .iter()
            .any(|entry| entry.path == "gates/judge.json")
        {
            result.reasons.push("protected dev lacks judge ABI: human-reviewed policy migration required; candidate policy not adopted".into());
            return Ok(result);
        }
        let abi = Abi::parse(&fs::read(regular(policy.root(), "gates/judge.json")?)?)?;
        let registry = Registry::load(policy.root())?;
        result.canonical_registry_hash = Some(registry.fingerprint()?);
        let classification = registry.classify(paths);
        let plan = registry.plan(cadence, &classification.suites)?;
        result.gates = plan.gates;
        for gate in &result.gates {
            if !abi.dispatch.iter().any(|command| command.gate == *gate) {
                result.reasons.push(format!(
                    "protected ABI has no supported dispatch for {gate}"
                ));
            }
        }
        for pin in &abi.images {
            if observed_images.get(&pin.reference) != Some(&pin.actual_id) {
                result.reasons.push(format!(
                    "image {} missing or actual config ID differs from protected pin",
                    pin.reference
                ));
            }
        }
        result.image_pins = abi.images;
        if result.reasons.is_empty() {
            result.status = Status::PreparedNonAuthoritative;
        }
        // Preparation never attests isolation or executes any gate. No flag or
        // matching checksum changes authoritative:false to true.
        policy.verify()?;
        Ok(result)
    }
}
