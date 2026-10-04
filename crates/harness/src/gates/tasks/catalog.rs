use super::*;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Role {
    Coordinator,
    Tester,
    Implementer,
    Reviewer,
    Maintainer,
    Qa,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RoleSpec {
    pub(super) role: Role,
    pub(super) guides: Vec<String>,
    pub(super) activities: Vec<Activity>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Activity {
    Plan,
    Implement,
    Test,
    Review,
    Maintain,
    ObserveQa,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    version: u16,
    roles: Vec<RoleSpec>,
}
impl Catalog {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 16384 {
            return Err("role catalog exceeds 16KiB".into());
        }
        let catalog: Self = serde_json::from_slice(bytes)?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub(super) fn validate(&self) -> Result<()> {
        if self.version != 1 || self.roles.len() != 6 {
            return Err("role catalog v1 requires exactly six closed roles".into());
        }
        let mut seen = BTreeSet::new();
        for spec in &self.roles {
            if !seen.insert(spec.role) || spec.guides.is_empty() || spec.guides.len() > 8 {
                return Err("invalid/duplicate role or empty guidance".into());
            }
            let mut guides = BTreeSet::new();
            for path in &spec.guides {
                if !relative(path) || !guides.insert(path) || !guide(path) {
                    return Err(format!("noncanonical role guide {path}").into());
                }
            }
            let expected = match spec.role {
                Role::Coordinator => Activity::Plan,
                Role::Tester => Activity::Test,
                Role::Implementer => Activity::Implement,
                Role::Reviewer => Activity::Review,
                Role::Maintainer => Activity::Maintain,
                Role::Qa => Activity::ObserveQa,
            };
            if spec.activities != [expected] {
                return Err("roles cannot widen closed activity catalog".into());
            }
        }
        Ok(())
    }
    pub(super) fn role(&self, role: Role) -> Result<&RoleSpec> {
        self.roles
            .iter()
            .find(|spec| spec.role == role)
            .ok_or_else(|| "role absent".into())
    }
    /// Canonical v1: role/guidance set order normalized, all semantic fields bound.
    /// This checksum does not authenticate mutable candidate catalog ownership.
    pub(super) fn fingerprint(&self) -> Result<String> {
        self.validate()?;
        let mut canonical = self.clone();
        canonical.roles.sort_by_key(|spec| spec.role);
        for role in &mut canonical.roles {
            role.guides.sort();
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-task-role-catalog-v1\0");
        hash.update(&serde_json::to_vec(&canonical)?);
        Ok(format!(
            "blake3:role-catalog-canonical-v1:{}",
            hash.finalize().to_hex()
        ))
    }
}
fn guide(path: &str) -> bool {
    matches!(
        path,
        "AGENTS.md"
            | "docs/agent-engineering.md"
            | "docs/testing.md"
            | "docs/qa.md"
            | "docs/adr/0006-provider-neutral-harness.md"
            | "skills/harness-ci/SKILL.md"
            | "skills/performance/SKILL.md"
            | "skills/protocol/SKILL.md"
            | "skills/release/SKILL.md"
            | "skills/simulation-server/SKILL.md"
            | "skills/wasm-rendering/SKILL.md"
            | "skills/game-assets/SKILL.md"
            | "skills/asset-import/SKILL.md"
    )
}
