//! Measured bounded QA evidence bytes, never a QA execution or authority verdict.
use super::{Report, Status, validate};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::Path,
};
mod input;
mod io;
pub(in crate::qa) use input::ExpectedSource;
use io::Held;
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const REPORT_BYTES: usize = 4 * 1024 * 1024;
const ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;
const TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const ARTIFACTS: usize = 128;
const ALGORITHM: &str = "blake3:aoeworld-qa-evidence-v1";
const DOMAIN: &[u8] = b"blake3:aoeworld-qa-evidence-v1\0";

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct RawHash {
    pub(crate) algorithm: &'static str,
    pub(crate) bytes: u64,
    pub(crate) raw_blake3: String,
}
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Purpose {
    Journey { name: String },
    Finding { index: usize },
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct Artifact {
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) raw_blake3: String,
    pub(crate) purposes: Vec<Purpose>,
}
#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct Observation {
    pub(crate) schema: u16,
    pub(crate) algorithm: &'static str,
    pub(crate) assessment: &'static str,
    pub(crate) claimed_status: Status,
    pub(crate) authoritative: bool,
    pub(crate) independent_qa: &'static str,
    pub(crate) journey_execution: &'static str,
    pub(crate) source_identity: &'static str,
    pub(crate) served_build_binding: &'static str,
    pub(crate) report: RawHash,
    pub(crate) artifacts: Vec<Artifact>,
    pub(crate) manifest_blake3: String,
    pub(crate) limits: [&'static str; 4],
}
#[derive(Serialize)]
struct Payload<'a> {
    schema: u16,
    algorithm: &'static str,
    report: &'a RawHash,
    artifacts: &'a [Artifact],
}
fn manifest(report: &RawHash, artifacts: &[Artifact]) -> Result<String> {
    let payload = serde_json::to_vec(&Payload {
        schema: 1,
        algorithm: ALGORITHM,
        report,
        artifacts,
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN);
    hasher.update(&payload);
    Ok(hasher.finalize().to_hex().to_string())
}

pub(in crate::qa) struct RetainedInputs {
    root: std::path::PathBuf,
    report_file: Held,
    files: Vec<Held>,
    pub(in crate::qa) observation: Observation,
    pub(in crate::qa) expected_source: Option<ExpectedSource>,
}
impl RetainedInputs {
    pub(in crate::qa) fn recheck_all(&mut self) -> Result<()> {
        io::root(&self.root)?;
        for (file, artifact) in self.files.iter_mut().zip(&self.observation.artifacts) {
            file.recheck(&artifact.raw_blake3)?;
        }
        self.report_file
            .recheck(&self.observation.report.raw_blake3)?;
        Ok(())
    }
}
pub(crate) fn observe_file_at(path: &Path, root: &Path) -> Result<Observation> {
    let mut retained = retain_file_at(path, root, false)?;
    retained.recheck_all()?;
    Ok(retained.observation)
}
pub(in crate::qa) fn retain_file_at(
    path: &Path,
    root: &Path,
    allow_source: bool,
) -> Result<RetainedInputs> {
    let root = io::root(root)?;
    let mut report_file = Held::open(path, REPORT_BYTES as u64)?;
    let bytes = report_file.read()?;
    let (report, expected_source) = input::parse(&bytes, allow_source)?;
    validate(&report).map_err(|error| -> Box<dyn Error> { error.into() })?;
    let report_hash = RawHash {
        algorithm: "blake3-raw-v1",
        bytes: bytes.len() as u64,
        raw_blake3: blake3::hash(&bytes).to_hex().to_string(),
    };
    let mut inventory: BTreeMap<String, (Held, Artifact)> = BTreeMap::new();
    let mut total = 0u64;
    let references = report
        .journeys
        .iter()
        .map(|journey| {
            (
                Purpose::Journey {
                    name: journey.name.clone(),
                },
                &journey.evidence,
            )
        })
        .chain(
            report
                .findings
                .iter()
                .enumerate()
                .map(|(index, finding)| (Purpose::Finding { index }, &finding.evidence)),
        );
    for (purpose, paths) in references {
        let mut seen = BTreeSet::new();
        for path in paths {
            let path = io::absolute(Path::new(path))?;
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| "QA evidence escapes the evidence root")?;
            let relative = relative
                .to_str()
                .ok_or("QA artifact path must be UTF-8")?
                .to_string();
            if relative.is_empty() || !seen.insert(relative.clone()) {
                return Err("duplicate QA evidence reference within one purpose".into());
            }
            if let Some((_, artifact)) = inventory.get_mut(&relative) {
                artifact.purposes.push(purpose.clone());
                continue;
            }
            if inventory.len() == ARTIFACTS {
                return Err("QA unique artifact count exceeds limit".into());
            }
            let mut file = Held::open(&path, ARTIFACT_BYTES)?;
            if file.same_file(&report_file) || file.path() == report_file.path() {
                return Err("QA report cannot be its own evidence artifact".into());
            }
            total = total
                .checked_add(file.length())
                .ok_or("QA aggregate byte count overflow")?;
            if total > TOTAL_BYTES {
                return Err("QA aggregate evidence bytes exceed limit".into());
            }
            let raw_blake3 = file.digest()?;
            let artifact = Artifact {
                path: relative.clone(),
                bytes: file.length(),
                raw_blake3,
                purposes: vec![purpose.clone()],
            };
            inventory.insert(relative, (file, artifact));
        }
    }
    io::root(&root)?;
    for (file, artifact) in inventory.values_mut() {
        file.recheck(&artifact.raw_blake3)?;
        artifact.purposes.sort();
    }
    report_file.recheck(&report_hash.raw_blake3)?;
    let (files, artifacts): (Vec<_>, Vec<_>) = inventory.into_values().unzip();
    let manifest_blake3 = manifest(&report_hash, &artifacts)?;
    let observation = Observation {
        schema: 1,
        algorithm: ALGORITHM,
        assessment: "STRUCTURAL_EVIDENCE_OBSERVED_NON_AUTHORITATIVE",
        claimed_status: report.status,
        authoritative: false,
        independent_qa: "NOT_ASSESSED",
        journey_execution: "NOT_ASSESSED",
        source_identity: "UNAVAILABLE",
        served_build_binding: "UNAVAILABLE",
        report: report_hash,
        artifacts,
        manifest_blake3,
        limits: [
            "shared evidence bytes do not prove each journey executed",
            "binary bytes do not establish screenshot meaning or running build identity",
            "endpoint observations cannot detect every reverted hostile race or impose wall deadlines",
            "local hashes and claimed status do not authenticate independent QA or authorize submission",
        ],
    };
    Ok(RetainedInputs {
        root,
        report_file,
        files,
        observation,
        expected_source,
    })
}
#[cfg(test)]
mod tests;
