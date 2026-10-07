use super::*;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Provider {
    Codex,
    PiDev,
    DeepSeekHarness,
    ClaudeCode,
}
/// Task planning records provider unavailability; it never launches a provider.
#[derive(Debug)]
pub(super) enum Observation {
    Unavailable(String),
}
#[derive(Debug, Serialize)]
pub(super) struct Descriptor {
    provider: Provider,
    availability: &'static str,
    version: Option<String>,
    pre_tool_interception: &'static str,
    filesystem_isolation: &'static str,
    authoritative_role_identity: bool,
    reason: String,
}
pub(super) fn describe(provider: Provider, observed: Observation) -> Result<Descriptor> {
    let Observation::Unavailable(reason) = observed;
    if !bounded(&reason, 2048) {
        return Err("invalid provider-unavailability reason".into());
    }
    Ok(Descriptor {
        provider,
        availability: "UNAVAILABLE",
        version: None,
        pre_tool_interception: "UNAVAILABLE: no SDK hook integration measured",
        filesystem_isolation: "UNAVAILABLE: no restricted launcher measured",
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
