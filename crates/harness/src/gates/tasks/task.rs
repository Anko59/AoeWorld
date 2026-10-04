use super::adapters::Provider;
use super::*;
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Kind {
    Feature,
    Bug,
    Refactor,
    Performance,
    Geodata,
    Qa,
    ReleaseInspection,
    PolicyUpgrade,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Status {
    Planned,
    Implementing,
    Testing,
    Reviewing,
    Maintaining,
    Qa,
    Blocked,
    ReadyForHuman,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ArtifactKind {
    TestEvidence,
    Review,
    QaObservation,
    Benchmark,
    TaskState,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub(super) kind: ArtifactKind,
    pub(super) path: String,
    pub(super) blake3: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Task {
    pub(super) version: u16,
    pub(super) id: String,
    pub(super) kind: Kind,
    pub(super) candidate: String,
    pub(super) base: String,
    pub(super) registry_hash: String,
    pub(super) role: Role,
    pub(super) provider: Provider,
    pub(super) status: Status,
    pub(super) objective: String,
    pub(super) acceptance: Vec<String>,
    pub(super) todo: Vec<String>,
    pub(super) artifacts: Vec<Artifact>,
    pub(super) rounds_remaining: u16,
}
#[derive(Debug, Serialize)]
pub(super) struct Handoff {
    actor: Role,
    next: Status,
}
impl Task {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 32768 {
            return Err("task JSON exceeds 32KiB".into());
        }
        let task: Self = serde_json::from_slice(bytes)?;
        task.validate(&task.registry_hash)?;
        Ok(task)
    }
    /// Unverified semantic handoff only. Actor and task JSON are NOT identity proof.
    pub(super) fn transition(&mut self, actor: Role, next: Status) -> Result<()> {
        self.validate(&self.registry_hash)?;
        let allowed = match (self.status, next, actor) {
            (_, Status::Blocked, _) => true,
            (Status::Blocked, Status::Planned, Role::Coordinator) => self.rounds_remaining > 0,
            (Status::Planned, Status::Implementing, Role::Implementer) => true,
            (Status::Planned, Status::Maintaining, Role::Maintainer) => true,
            (
                Status::Planned | Status::Implementing | Status::Maintaining,
                Status::Testing,
                Role::Tester,
            ) => true,
            (Status::Testing, Status::Reviewing, Role::Reviewer) => true,
            (Status::Reviewing, Status::Qa, Role::Qa) => true,
            (Status::Reviewing | Status::Qa, Status::ReadyForHuman, Role::Coordinator) => true,
            _ => false,
        };
        if !allowed || (self.rounds_remaining == 0 && next != Status::Blocked) {
            return Err("invalid task-state handoff or exhausted round budget".into());
        }
        self.status = next;
        self.role = actor;
        Ok(())
    }
    pub(super) fn semantic_handoffs(&self) -> Vec<Handoff> {
        [
            (Role::Implementer, Status::Implementing),
            (Role::Maintainer, Status::Maintaining),
            (Role::Tester, Status::Testing),
            (Role::Reviewer, Status::Reviewing),
            (Role::Qa, Status::Qa),
            (Role::Coordinator, Status::ReadyForHuman),
            (Role::Coordinator, Status::Blocked),
            (Role::Coordinator, Status::Planned),
        ]
        .into_iter()
        .filter_map(|(actor, next)| {
            let mut proposed = self.clone();
            proposed
                .transition(actor, next)
                .ok()
                .map(|()| Handoff { actor, next })
        })
        .collect()
    }
    pub(super) fn validate(&self, expected_registry: &str) -> Result<()> {
        if self.version != 1 || !identifier(&self.id) || !oid(&self.candidate) || !oid(&self.base) {
            return Err("task requires v1, bounded id and resolved lowercase full OIDs".into());
        }
        if !self
            .registry_hash
            .strip_prefix("blake3:registry-v2-canonical-v1:")
            .is_some_and(hex_digest)
            || self.registry_hash != expected_registry
        {
            return Err("task registry hash malformed or changed; re-plan exact task".into());
        }
        let expected = match self.status {
            Status::Planned | Status::ReadyForHuman => Some(Role::Coordinator),
            Status::Implementing => Some(Role::Implementer),
            Status::Testing => Some(Role::Tester),
            Status::Reviewing => Some(Role::Reviewer),
            Status::Maintaining => Some(Role::Maintainer),
            Status::Qa => Some(Role::Qa),
            Status::Blocked => None,
        };
        if expected.is_some_and(|role| role != self.role) {
            return Err("task status and semantic role disagree".into());
        }
        if self.rounds_remaining > 256 {
            return Err("round budget exceeds 256".into());
        }
        if self.rounds_remaining == 0 && self.status != Status::Blocked {
            return Err("exhausted task must record blocked state".into());
        }
        text(&self.objective, 2048)?;
        for list in [&self.acceptance, &self.todo] {
            if list.is_empty() || list.len() > 16 {
                return Err("acceptance/todo requires 1..16 bounded items".into());
            }
            for item in list {
                text(item, 512)?;
            }
        }
        if self.artifacts.len() > 16 {
            return Err("artifact count exceeds 16".into());
        }
        let namespace = format!("task-artifacts/{}/", self.id);
        let mut paths = BTreeSet::new();
        for artifact in &self.artifacts {
            if !relative(&artifact.path)
                || !artifact.path.starts_with(&namespace)
                || !paths.insert(&artifact.path)
            {
                return Err("artifact requires unique exact logical task namespace".into());
            }
            if artifact
                .blake3
                .as_ref()
                .is_some_and(|hash| !hex_digest(hash))
            {
                return Err("artifact digest must be lowercase BLAKE3 hex".into());
            }
        }
        Ok(())
    }
}
fn text(value: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > limit
        || value
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && ch != '\n' && ch != '\t'))
    {
        return Err("invalid or oversized model-facing text".into());
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub(super) struct ContextPacket {
    pub(super) task_id: String,
    pub(super) candidate: String,
    pub(super) base: String,
    pub(super) role: Role,
    pub(super) objective: String,
    pub(super) acceptance: Vec<String>,
    pub(super) guides: Vec<String>,
    pub(super) max_utf8_bytes: usize,
}
impl ContextPacket {
    pub(super) fn new(task: &Task, role: &catalog::RoleSpec) -> Result<Self> {
        let mut guides: BTreeSet<String> = role.guides.iter().cloned().collect();
        guides.extend(["docs/agent-engineering.md".into(), "docs/testing.md".into()]);
        match task.kind {
            Kind::Feature | Kind::Bug | Kind::Refactor => {
                guides.insert("skills/protocol/SKILL.md".into());
            }
            Kind::Performance => {
                guides.insert("skills/performance/SKILL.md".into());
            }
            Kind::Geodata => {
                guides.extend([
                    "skills/asset-import/SKILL.md".into(),
                    "skills/game-assets/SKILL.md".into(),
                ]);
            }
            Kind::Qa => {
                guides.insert("skills/game-assets/SKILL.md".into());
            }
            Kind::ReleaseInspection => {
                guides.insert("skills/release/SKILL.md".into());
            }
            Kind::PolicyUpgrade => {
                guides.extend([
                    "skills/harness-ci/SKILL.md".into(),
                    "docs/adr/0006-provider-neutral-harness.md".into(),
                ]);
            }
        }
        let packet = Self {
            task_id: task.id.clone(),
            candidate: task.candidate.clone(),
            base: task.base.clone(),
            role: task.role,
            objective: task.objective.clone(),
            acceptance: task.acceptance.clone(),
            guides: guides.into_iter().collect(),
            max_utf8_bytes: 16384,
        };
        if serde_json::to_vec(&packet)?.len() > packet.max_utf8_bytes {
            return Err("context packet exceeds 16KiB".into());
        }
        Ok(packet)
    }
}
