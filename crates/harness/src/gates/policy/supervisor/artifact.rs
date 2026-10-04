//! Fixed-algorithm signature observations, never independent service authority.
mod key;
#[cfg(test)]
mod tests;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
const SUBJECT_DOMAIN: &[u8] = b"aoeworld:protected-artifact-subject:v1\0";
const KEY_DOMAIN: &[u8] = b"aoeworld:artifact-key:v1\0";
const KEY_PATH: &str = "/etc/aoeworld/supervisor/trust-key.json";
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Envelope {
    schema: u32,
    key_id: String,
    signature: String,
}
fn hex<const N: usize>(value: &str) -> Result<[u8; N], &'static str> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("expected fixed lowercase hex");
    }
    let mut result = [0; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let digit = |c: u8| if c <= b'9' { c - b'0' } else { c - b'a' + 10 };
        result[index] = digit(pair[0]) * 16 + digit(pair[1]);
    }
    Ok(result)
}
fn key_id(bytes: &[u8; 32]) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(KEY_DOMAIN);
    hash.update(bytes);
    hash.finalize().to_hex().to_string()
}
fn message(subject: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SUBJECT_DOMAIN.len() + subject.len());
    bytes.extend_from_slice(SUBJECT_DOMAIN);
    bytes.extend_from_slice(subject);
    bytes
}
fn report(status: &str, reason: &str) -> Value {
    json!({"status":status,"reason":reason,"authoritative":false,"admission_granted":false,
        "limits":["root-owned key bytes are an observation, not independent operator/deployment identity","same-user/reverted races, ACLs and container-root namespaces remain outside this proof","coding-host daemon control can rewrite root-owned state; no service authority granted","subject fields remain assertions: executable/runtime/build/container bytes are not inspected"]})
}
pub(super) fn inspect(
    subject_bytes: &[u8],
    repository_id: u64,
    trust_root_id: &str,
    envelope: Option<&Envelope>,
) -> Value {
    evaluate(
        subject_bytes,
        repository_id,
        trust_root_id,
        envelope,
        key::fixed,
    )
}
fn evaluate(
    subject: &[u8],
    repository_id: u64,
    trust_root_id: &str,
    envelope: Option<&Envelope>,
    load: impl FnOnce() -> Result<key::Observed, &'static str>,
) -> Value {
    let Some(envelope) = envelope else {
        return report("ABSENT", "artifact signature absent; key not consulted");
    };
    if subject.len() > 16384
        || envelope.schema != 1
        || hex::<32>(&envelope.key_id).is_err()
        || trust_root_id != envelope.key_id
    {
        return report(
            "REJECTED",
            "artifact subject/envelope bounds or schema invalid",
        );
    }
    let signature = match hex::<64>(&envelope.signature) {
        Ok(bytes) => Signature::from_bytes(&bytes),
        Err(_) => {
            return report(
                "REJECTED",
                "artifact signature must be exactly128 lowercase hex",
            );
        }
    };
    let observed = match load() {
        Ok(value) => value,
        Err(reason) => return report("UNAVAILABLE_KEY", reason),
    };
    if observed.repository_id != repository_id || observed.key_id != envelope.key_id {
        return report(
            "REJECTED",
            "artifact key repository or key identity mismatch",
        );
    }
    let public = match VerifyingKey::from_bytes(&observed.public_key) {
        Ok(value) => value,
        Err(_) => return report("REJECTED", "invalid Ed25519 verification key"),
    };
    if public.is_weak() || public.verify_strict(&message(subject), &signature).is_err() {
        return report("REJECTED", "strict Ed25519 artifact signature rejected");
    }
    let mut result = report(
        "VERIFIED_SIGNATURE_NON_AUTHORITATIVE",
        "signature matches observed root-key bytes; independent service admission unavailable",
    );
    result["subject_blake3"] = json!(blake3::hash(subject).to_hex().to_string());
    result["key_file_blake3"] = json!(observed.file_blake3);
    result["key_id"] = json!(observed.key_id);
    result
}
