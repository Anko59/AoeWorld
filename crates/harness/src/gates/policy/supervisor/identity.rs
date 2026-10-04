//! Internal correlation identities; neither labels nor hashes authenticate a service.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct ContainerId(String);
impl ContainerId {
    pub(super) fn simulation() -> Self {
        Self("a".repeat(64))
    }
    pub(super) fn observed(bytes: &[u8]) -> Result<Self, &'static str> {
        let text = std::str::from_utf8(bytes).map_err(|_| "CID is not UTF8")?;
        let text = text.strip_suffix('\n').unwrap_or(text);
        if text.len() != 64
            || !text
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("expected one full lowercase 64-character observed CID");
        }
        Ok(Self(text.to_owned()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct ExpectedIdentity {
    service: String,
    epoch: String,
    nonce: String,
    daemon: String,
    image_config: String,
    security: String,
}
impl ExpectedIdentity {
    // No request deserialization creates service authority. This fixed constructor
    // belongs only to deterministic models, not deployment qualification.
    pub(super) fn simulation() -> Self {
        Self {
            service: "model-service".into(),
            epoch: "model-epoch".into(),
            nonce: "model-owned-nonce".into(),
            daemon: "model-daemon".into(),
            image_config: format!("sha256:{}", "2".repeat(64)),
            security: "uid65532:network-none:readonly:capdrop-all:fixed-mounts:bounded".into(),
        }
    }
    pub(super) fn changed(&self, field: &str) -> Self {
        let mut value = self.clone();
        match field {
            "service" => value.service.push('x'),
            "epoch" => value.epoch.push('x'),
            "nonce" => value.nonce.push('x'),
            "daemon" => value.daemon.push('x'),
            "image" => value.image_config.push('x'),
            _ => value.security.push('x'),
        }
        value
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Observation {
    pub(super) cid: String,
    pub(super) identity: serde_json::Value,
}
impl Observation {
    pub(super) fn matching(id: &ContainerId, expected: &ExpectedIdentity) -> Self {
        Self {
            cid: id.0.clone(),
            identity: serde_json::to_value(expected).unwrap_or(serde_json::Value::Null),
        }
    }
    pub(super) fn verify(&self, id: &ContainerId, expected: &ExpectedIdentity) -> bool {
        self.cid == id.0 && serde_json::to_value(expected).is_ok_and(|value| self.identity == value)
    }
}
