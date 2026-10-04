use super::{MAX_BYTES, Registry, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    pub requirements: Vec<Requirement>,
    pub cases: Vec<Case>,
    pub deferred: Vec<Deferred>,
    schema: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Requirement {
    pub id: String,
    risk: Risk,
    scope: String,
    statement: String,
    pub invariants: Vec<Invariant>,
    gate_refs: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Risk {
    Critical,
    High,
    Normal,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Invariant {
    pub id: String,
    claim: String,
    pub boundaries: Vec<Boundary>,
    case_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Boundary {
    pub path: String,
    pub symbol: String,
    pub kind: BoundaryKind,
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BoundaryKind {
    Function,
    Method,
}
impl BoundaryKind {
    pub fn name(&self) -> &str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Case {
    pub id: String,
    pub kind: Purpose,
    pub path: String,
    pub symbol: String,
    runner: Runner,
    gate: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Purpose {
    Positive,
    Negative,
}
impl Purpose {
    pub fn name(&self) -> &str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Runner {
    NativeTest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Deferred {
    pub id: String,
    pub capability: Capability,
    pub reason: String,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Capability {
    SourceAssets,
    SourceGeodata,
}
fn text(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err("contract text must be nonempty bounded printable text".into());
    }
    Ok(())
}
fn id(value: &str, used: &mut BTreeSet<String>) -> Result<()> {
    if value.is_empty()
        || value.len() > 96
        || !value.as_bytes()[0].is_ascii_lowercase()
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || !used.insert(value.into())
    {
        return Err("invalid or duplicate contract identifier".into());
    }
    Ok(())
}
fn binding(path: &str, symbol: &str) -> Result<()> {
    if !path.starts_with("crates/")
        || !path.ends_with(".rs")
        || path.len() > 256
        || path.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
    {
        return Err("contract source path must be normal repository Rust path".into());
    }
    if symbol.is_empty()
        || symbol.len() > 256
        || symbol.split("::").any(|part| {
            part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err("contract symbol must be a bounded Rust item path".into());
    }
    Ok(())
}
fn unique_refs(values: &[String]) -> Result<BTreeSet<String>> {
    let result: BTreeSet<_> = values.iter().cloned().collect();
    if result.is_empty() || result.len() != values.len() || values.len() > 64 {
        return Err("contract references must be nonempty bounded unique sets".into());
    }
    Ok(result)
}
impl Catalog {
    pub(super) fn parse(bytes: &[u8], registry: &Registry) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err("contract catalog exceeds128KiB".into());
        }
        let catalog: Self = serde_json::from_slice(bytes)?;
        catalog.validate(registry)?;
        Ok(catalog)
    }
    fn validate(&self, registry: &Registry) -> Result<()> {
        if self.schema != 1
            || self.requirements.is_empty()
            || self.requirements.len() > 64
            || self.cases.is_empty()
            || self.cases.len() > 256
            || self.deferred.len() > 32
        {
            return Err("invalid contract schema/count bounds".into());
        }
        let known: BTreeSet<_> = registry.gates.iter().map(|gate| gate.id.as_str()).collect();
        let mut ids = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let mut cases = BTreeMap::new();
        for case in &self.cases {
            id(&case.id, &mut ids)?;
            binding(&case.path, &case.symbol)?;
            if !sources.insert((&case.path, &case.symbol))
                || case.gate != "test-unit"
                || !known.contains(case.gate.as_str())
                || !matches!(case.runner, Runner::NativeTest)
            {
                return Err(
                    "case must bind a unique source symbol to known native test-unit gate".into(),
                );
            }
            cases.insert(case.id.as_str(), case);
        }
        let mut referenced = BTreeSet::new();
        for requirement in &self.requirements {
            id(&requirement.id, &mut ids)?;
            text(&requirement.scope)?;
            text(&requirement.statement)?;
            let gates = unique_refs(&requirement.gate_refs)?;
            if requirement.invariants.is_empty()
                || requirement.invariants.len() > 32
                || gates.iter().any(|gate| !known.contains(gate.as_str()))
            {
                return Err("invalid requirement invariants/gates".into());
            }
            let mut expected_gates = BTreeSet::new();
            let mut positive = false;
            let mut negative = false;
            for invariant in &requirement.invariants {
                id(&invariant.id, &mut ids)?;
                text(&invariant.claim)?;
                if invariant.boundaries.is_empty() || invariant.boundaries.len() > 16 {
                    return Err("invariant requires bounded production boundaries".into());
                }
                let mut boundaries = BTreeSet::new();
                for boundary in &invariant.boundaries {
                    binding(&boundary.path, &boundary.symbol)?;
                    if !boundaries.insert((&boundary.path, &boundary.symbol, boundary.kind.name()))
                    {
                        return Err("duplicate invariant boundary".into());
                    }
                }
                for case_id in unique_refs(&invariant.case_ids)? {
                    let case = cases
                        .get(case_id.as_str())
                        .ok_or("unknown invariant case reference")?;
                    referenced.insert(case_id);
                    expected_gates.insert(case.gate.clone());
                    positive |= matches!(case.kind, Purpose::Positive);
                    negative |= matches!(case.kind, Purpose::Negative);
                }
            }
            if gates != expected_gates
                || !(matches!(requirement.risk, Risk::Normal) || positive && negative)
            {
                return Err("requirement gates must exactly bind cases; high/critical requires both claimed purposes".into());
            }
        }
        if referenced.len() != cases.len() {
            return Err("unreferenced contract cases".into());
        }
        for item in &self.deferred {
            id(&item.id, &mut ids)?;
            text(&item.reason)?;
        }
        Ok(())
    }
}
