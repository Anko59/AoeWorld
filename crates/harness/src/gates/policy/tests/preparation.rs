use super::*;

fn supported() -> (tempfile::TempDir, Materialized, BTreeMap<String, String>) {
    let owner = tempfile::tempdir().unwrap();
    fs::create_dir(owner.path().join("gates")).unwrap();
    let registry = serde_json::json!({"version":2,"suites":[{"id":"everything","paths":["**"],"implies":["static"],"review":true},{"id":"static","paths":["**"],"implies":[],"review":false}],"gates":[{"id":"fmt-check","command":"make fmt-check","requires":[],"select":"fixture","evidence":"log","suites":["static"],"cadences":["pr","ci"],"budget_s":2,"static":true,"capabilities":[],"blocks":["pr"]}],"jobs":{"static":["fmt-check"]}});
    let reference = format!(
        "ghcr.io/anko59/aoeworld/rust-tools@sha256:{}",
        "1".repeat(64)
    );
    let id = format!("sha256:{}", "2".repeat(64));
    let abi = serde_json::json!({"schema":1,"abi":1,"registry_schema":2,"images":[{"name":"rust-tools","reference":reference,"actual_id":id}],"dispatch":[{"gate":"fmt-check","operation":"fmt-check","image":"rust-tools"}]});
    for (path, value) in [("gates/judge.json", abi), ("gates/registry.json", registry)] {
        fs::write(owner.path().join(path), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let entries = ["gates/judge.json", "gates/registry.json"]
        .into_iter()
        .map(|path| Entry {
            path: path.into(),
            mode: "100644".into(),
            blob: "c".repeat(40),
            blake3: blake3::hash(&fs::read(owner.path().join(path)).unwrap())
                .to_hex()
                .to_string(),
        })
        .collect();
    let policy =
        Materialized::new(owner.path(), Closure::bind(source(), entries).unwrap()).unwrap();
    (owner, policy, BTreeMap::from([(reference, id)]))
}
#[test]
fn supported_policy_pins_prepare_only_with_matching_actual_images_and_complete_dispatch() {
    let (_owner, policy, images) = supported();
    let prepared = Preparation::prepare(
        &policy,
        &"d".repeat(40),
        &"e".repeat(40),
        &["unknown.rs".into()],
        Cadence::Pr,
        &images,
    )
    .unwrap();
    assert!(matches!(prepared.status, Status::PreparedNonAuthoritative));
    assert!(!prepared.authoritative);
    assert_eq!(prepared.gates, ["fmt-check"]);
    assert!(prepared.canonical_registry_hash.is_some());
    assert!(matches!(
        Preparation::prepare(
            &policy,
            &"d".repeat(40),
            &"e".repeat(40),
            &[],
            Cadence::Pr,
            &BTreeMap::new()
        )
        .unwrap()
        .status,
        Status::Unavailable
    ));
    let changed = images
        .into_keys()
        .map(|key| (key, format!("sha256:{}", "3".repeat(64))))
        .collect();
    assert!(matches!(
        Preparation::prepare(
            &policy,
            &"d".repeat(40),
            &"e".repeat(40),
            &[],
            Cadence::Pr,
            &changed
        )
        .unwrap()
        .status,
        Status::Unavailable
    ));
}
#[test]
fn closure_binds_all_trust_inputs_and_semantics_but_not_observation_timestamp() {
    let (owner, policy, _) = supported();
    let mut changed = policy.closure().source.clone();
    changed.observed_at_unix_s += 1;
    assert_eq!(
        Closure::bind(changed, policy.closure().entries.clone())
            .unwrap()
            .blake3,
        policy.closure().blake3
    );
    let mut changed = policy.closure().source.clone();
    changed.commit = "f".repeat(40);
    assert_ne!(
        Closure::bind(changed, policy.closure().entries.clone())
            .unwrap()
            .blake3,
        policy.closure().blake3
    );
    fs::write(owner.path().join("build.rs"), "fn main() {}\n").unwrap();
    assert!(policy.verify().is_err());
}
#[test]
fn missing_selected_dispatch_and_unsupported_abi_never_drop_checks_or_prepare() {
    let (owner, policy, images) = supported();
    let path = owner.path().join("gates/judge.json");
    let mut abi: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    abi["dispatch"] = serde_json::json!([]);
    fs::write(&path, serde_json::to_vec(&abi).unwrap()).unwrap();
    let rebind = || {
        let entries = policy
            .closure()
            .entries
            .iter()
            .map(|entry| {
                let mut entry = entry.clone();
                entry.blake3 = blake3::hash(&fs::read(owner.path().join(&entry.path)).unwrap())
                    .to_hex()
                    .to_string();
                entry
            })
            .collect();
        Materialized::new(owner.path(), Closure::bind(source(), entries).unwrap()).unwrap()
    };
    let prepared = Preparation::prepare(
        &rebind(),
        &"d".repeat(40),
        &"e".repeat(40),
        &[],
        Cadence::Pr,
        &images,
    )
    .unwrap();
    assert!(matches!(prepared.status, Status::Unavailable));
    assert_eq!(prepared.gates, ["fmt-check"]);
    assert!(
        prepared
            .reasons
            .iter()
            .any(|reason| reason.contains("dispatch"))
    );
    abi["abi"] = serde_json::json!(2);
    fs::write(&path, serde_json::to_vec(&abi).unwrap()).unwrap();
    assert!(
        Preparation::prepare(
            &rebind(),
            &"d".repeat(40),
            &"e".repeat(40),
            &[],
            Cadence::Pr,
            &images
        )
        .is_err()
    );
}
#[cfg(unix)]
#[test]
fn materialization_rejects_absolute_symlink_ancestor_and_hardlink_alias() {
    use std::os::unix::fs::symlink;
    let (owner, policy) = fixture();
    let alias = tempfile::tempdir().unwrap();
    symlink(owner.path(), alias.path().join("policy")).unwrap();
    assert!(Materialized::new(&alias.path().join("policy"), policy.closure().clone()).is_err());
    fs::hard_link(
        owner.path().join("Cargo.toml"),
        alias.path().join("hardlink"),
    )
    .unwrap();
    assert!(policy.verify().is_err());
}
