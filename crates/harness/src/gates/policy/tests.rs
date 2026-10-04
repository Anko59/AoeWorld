use super::*;
mod preparation;

#[test]
fn image_tags_wrong_ids_unknown_abi_fields_and_publisher_dispatch_reject() {
    let pin = serde_json::json!({"name":"rust-tools","reference":format!("ghcr.io/anko59/aoeworld/rust-tools@sha256:{}", "1".repeat(64)),"actual_id":format!("sha256:{}", "2".repeat(64))});
    let good = serde_json::json!({"schema":1,"abi":1,"registry_schema":2,"images":[pin],"dispatch":[{"gate":"fmt-check","operation":"fmt-check","image":"rust-tools"}]});
    assert!(Abi::parse(&serde_json::to_vec(&good).unwrap()).is_ok());
    let mut bad = good.clone();
    bad["images"][0]["reference"] = serde_json::json!("aoeworld/rust-tools:latest");
    assert!(Abi::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = good.clone();
    bad["images"][0]["actual_id"] = serde_json::json!("sha256:short");
    assert!(Abi::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = good.clone();
    bad["dispatch"][0]["operation"] = serde_json::json!("release-publish");
    assert!(Abi::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = good;
    bad["trusted"] = serde_json::json!(true);
    assert!(Abi::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
}

fn anchor() -> Anchor {
    Anchor {
        schema: 1,
        repository: "Anko59/AoeWorld".into(),
        repository_id: 42,
        remote_url: "https://github.com/Anko59/AoeWorld.git".into(),
        integration_branch: "dev".into(),
    }
}
fn observation() -> ObservedBranch {
    ObservedBranch {
        repository: "Anko59/AoeWorld".into(),
        repository_id: 42,
        branch: "dev".into(),
        protected: true,
        commit: "a".repeat(40),
        required_contexts: BTreeSet::from(["required".into()]),
        strict: true,
        observed_at_unix_s: 123,
    }
}
fn source() -> SourceIdentity {
    observation()
        .resolve(&anchor(), &"a".repeat(40), &"b".repeat(40))
        .unwrap()
}
fn fixture() -> (tempfile::TempDir, Materialized) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("Cargo.toml"), b"[workspace]\n").unwrap();
    let entry = Entry {
        path: "Cargo.toml".into(),
        mode: "100644".into(),
        blob: "c".repeat(40),
        blake3: blake3::hash(b"[workspace]\n").to_hex().to_string(),
    };
    let closure = Closure::bind(source(), vec![entry]).unwrap();
    let policy = Materialized::new(directory.path(), closure).unwrap();
    (directory, policy)
}
#[test]
fn source_requires_actual_fixed_observation_not_self_declared_flags() {
    assert!(Anchor::parse(br#"{"schema":1,"repository":"Anko59/AoeWorld","repository_id":42,"remote_url":"https://github.com/Anko59/AoeWorld.git","integration_branch":"dev","trusted":true}"#).is_err());
    for oid in [
        "HEAD",
        "-f",
        "abc123",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        assert!(full_oid(oid).is_err());
    }
    let mut api = observation();
    api.protected = false;
    assert!(
        api.resolve(&anchor(), &"a".repeat(40), &"b".repeat(40))
            .is_err()
    );
    assert!(
        observation()
            .resolve(&anchor(), &"d".repeat(40), &"b".repeat(40))
            .is_err()
    );
    let mut api = observation();
    api.repository_id = 99;
    assert!(
        api.resolve(&anchor(), &"a".repeat(40), &"b".repeat(40))
            .is_err()
    );
    assert!(
        observation()
            .resolve(&anchor(), &"a".repeat(40), &"b".repeat(40))
            .is_ok()
    );
}
#[test]
fn missing_legacy_base_abi_is_unavailable_before_registry_upgrade() {
    let (_owner, policy) = fixture();
    let result = Preparation::prepare(
        &policy,
        &"d".repeat(40),
        &"e".repeat(40),
        &[],
        Cadence::Edit,
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(matches!(result.status, Status::Unavailable));
    assert!(!result.authoritative);
    assert!(result.canonical_registry_hash.is_none());
    assert!(result.reasons[0].contains("migration"));
}
#[test]
fn changed_bytes_untracked_shadow_or_tampered_manifest_reject() {
    let (owner, policy) = fixture();
    fs::write(owner.path().join("Cargo.toml"), b"tampered\n").unwrap();
    assert!(policy.verify().is_err());
    let (owner, policy) = fixture();
    fs::create_dir(owner.path().join(".cargo")).unwrap();
    fs::write(
        owner.path().join(".cargo/config.toml"),
        b"[build]\nrustc-wrapper='attack'\n",
    )
    .unwrap();
    assert!(policy.verify().is_err());
    let (_owner, policy) = fixture();
    let mut json = serde_json::to_value(policy.closure()).unwrap();
    json["source"]["commit"] = serde_json::json!("f".repeat(40));
    assert!(Closure::parse(&serde_json::to_vec(&json).unwrap()).is_err());
    let mut json = serde_json::to_value(policy.closure()).unwrap();
    json["trusted"] = serde_json::json!(true);
    assert!(Closure::parse(&serde_json::to_vec(&json).unwrap()).is_err());
}
#[cfg(unix)]
#[test]
fn symlink_replacements_and_symlink_parents_reject() {
    use std::os::unix::fs::symlink;
    let (owner, policy) = fixture();
    let source = owner.path().join("Cargo.toml");
    assert_eq!(fs::canonicalize(&source).unwrap(), source);
    fs::remove_file(&source).unwrap();
    symlink("/etc/passwd", &source).unwrap();
    assert!(policy.verify().is_err());
    let owner = tempfile::tempdir().unwrap();
    symlink("/etc", owner.path().join("etc")).unwrap();
    assert!(regular(owner.path(), "etc/passwd").is_err());
}
