use super::*;

/// Validated semantic registry hash v1; not RFC8785/JCS. Set-like arrays are
/// sorted, map keys ordered, struct order stable; all execution policy is bound.
pub(super) fn fingerprint(registry: &Registry) -> Result<String> {
    let mut registry = Registry::parse(&serde_json::to_vec(registry)?)?;
    registry.suites.sort_by(|a, b| a.id.cmp(&b.id));
    for suite in &mut registry.suites {
        suite.paths.sort();
        suite.implies.sort();
    }
    registry.gates.sort_by(|a, b| a.id.cmp(&b.id));
    for gate in &mut registry.gates {
        gate.requires.sort();
        gate.suites.sort();
        gate.cadences.sort();
        gate.capabilities.sort();
        gate.blocks.sort();
    }
    for members in registry.jobs.values_mut() {
        members.sort();
    }
    let mut hash = blake3::Hasher::new();
    hash.update(b"aoe-registry-v2-canonical-v1\0");
    hash.update(&serde_json::to_vec(&registry)?);
    Ok(format!(
        "blake3:registry-v2-canonical-v1:{}",
        hash.finalize().to_hex()
    ))
}
