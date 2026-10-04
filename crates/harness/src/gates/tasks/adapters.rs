use super::*;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Provider {
    Codex,
    PiDev,
    DeepSeekHarness,
}
impl Provider {
    pub(super) fn executable(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::PiDev => "pi",
            Self::DeepSeekHarness => "dsh",
        }
    }
}
/// Constructed by a launcher's fixed read-only executable/version probe, NEVER
/// supplied as verified:true in task JSON. Availability is not interception.
#[derive(Debug)]
pub(super) enum Observation {
    Unavailable(String),
    Installed { executable: String, version: String },
}
#[derive(Debug, Serialize)]
pub(super) struct Descriptor {
    provider: Provider,
    invocation: Option<Invocation>,
    availability: String,
    version: Option<String>,
    pre_tool_interception: String,
    filesystem_isolation: String,
    authoritative_role_identity: bool,
    reason: String,
}
#[derive(Debug, Serialize)]
struct Invocation {
    executable: String,
    protocol: &'static str,
    canonical_operation: &'static str,
}
pub(super) fn describe(provider: Provider, observed: Observation) -> Result<Descriptor> {
    let (invocation, availability, version, reason) = match observed {
        Observation::Unavailable(reason) => {
            if !bounded(&reason, 2048) {
                return Err("invalid provider-unavailability reason".into());
            }
            (None, "UNAVAILABLE", None, reason)
        }
        Observation::Installed {
            executable,
            version,
        } => {
            if !bounded(&version, 1024)
                || !bounded(&executable, 512)
                || std::path::Path::new(&executable)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some(provider.executable())
            {
                return Err(
                    "provider observation mismatches fixed executable or bounded version".into(),
                );
            }
            (
                Some(Invocation {
                    executable,
                    protocol: "portable-task-v1",
                    canonical_operation: "aoe-harness task-plan",
                }),
                "OBSERVED_INSTALLED",
                Some(version),
                "Binary presence/version does not establish live hooks or enforcement".into(),
            )
        }
    };
    Ok(Descriptor {
        provider,
        invocation,
        availability: availability.into(),
        version,
        pre_tool_interception: "UNAVAILABLE: no SDK hook integration measured".into(),
        filesystem_isolation: "UNAVAILABLE: no restricted launcher measured".into(),
        authoritative_role_identity: false,
        reason,
    })
}
fn bounded(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= maximum
        && !value
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
}
