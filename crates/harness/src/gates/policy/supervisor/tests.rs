use super::*;
fn payload(root: &Path) -> Value {
    for name in ["artifact", "evidence", "leases", "candidate"] {
        fs::create_dir(root.join(name)).unwrap();
    }
    let operations = [
        Operation::FmtCheck,
        Operation::StructureCheck,
        Operation::ArchitectureCheck,
        Operation::DocsCheck,
        Operation::Lint,
        Operation::TestUnit,
    ];
    let dispatch: Vec<_> = operations
        .iter()
        .map(|op| json!({"gate":op.argument(),"operation":op.argument(),"image":"rust-tools"}))
        .collect();
    json!({"schema":1,"anchor":{"schema":1,"repository":"Example/Policy","repository_id":7,"remote_url":"https://github.com/Example/Policy.git","integration_branch":"dev"},
        "service_uid":71231,"candidate_uid":65532,"artifact_root":root.join("artifact"),"evidence_root":root.join("evidence"),"lease_root":root.join("leases"),
        "subject":{"schema":1,"repository_id":7,"protected_ref":"refs/heads/dev","protected_commit":"1".repeat(40),"protected_tree":"2".repeat(40),"closure_blake3":"3".repeat(64),"registry_hash":format!("blake3:registry-v2-canonical-v1:{}","4".repeat(64)),"executable_blake3":"5".repeat(64),"runtime_blake3":"6".repeat(64),"trust_root_id":"7".repeat(64),"abi":{"schema":1,"abi":1,"registry_schema":2,"images":[{"name":"rust-tools","reference":format!("ghcr.io/anko59/aoeworld/rust-tools@sha256:{}","8".repeat(64)),"actual_id":format!("sha256:{}","9".repeat(64))}],"dispatch":dispatch}},
        "resources":{"memory_mib":1024,"pids":128,"cpus":2,"workload_s":30,"cleanup_s":15,"command_s":5}})
}
fn parse(value: &Value) -> Result<requirements::Requirements> {
    requirements::Requirements::parse(&serde_json::to_vec(value)?)
}
#[test]
fn well_formed_unsigned_subject_never_mints_admission_or_execution() {
    let owner = tempfile::tempdir().unwrap();
    let value = payload(owner.path());
    let report = parse(&value)
        .unwrap()
        .report(&owner.path().join("candidate"))
        .unwrap();
    assert_eq!(report["status"], "UNAVAILABLE");
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["worker_template"]["execution"], "NOT_IMPLEMENTED");
    assert_eq!(report["worker_template"]["network"], "none");
    assert_eq!(report["worker_template"]["environment"], json!({}));
    assert_eq!(
        report["worker_template"]["operations"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(
        report["reasons"]
            .to_string()
            .contains("unverified assertions")
    );
    #[cfg(unix)]
    assert_eq!(
        report["root_observations"][0]["matches_declared_service_uid"],
        false
    );
}
#[test]
fn unknown_claims_wrong_domains_identity_and_abi_do_not_relax_requirements() {
    let owner = tempfile::tempdir().unwrap();
    let good = payload(owner.path());
    let cases = [
        ("/schema", json!(2)),
        ("/service_uid", json!(65532)),
        ("/candidate_uid", json!(0)),
        ("/subject/schema", json!(2)),
        ("/subject/repository_id", json!(8)),
        ("/subject/protected_ref", json!("refs/heads/main")),
        ("/subject/protected_commit", json!("HEAD")),
        ("/subject/protected_tree", json!("A".repeat(40))),
        ("/subject/closure_blake3", json!("x".repeat(64))),
        ("/subject/executable_blake3", json!("short")),
        ("/subject/runtime_blake3", json!("Z".repeat(64))),
        ("/subject/trust_root_id", json!("key-from-candidate")),
        ("/subject/registry_hash", json!("4".repeat(64))),
        ("/subject/abi/abi", json!(2)),
        ("/subject/abi/images", json!([])),
        ("/subject/abi/dispatch", json!([])),
        (
            "/subject/abi/dispatch/0/operation",
            json!("release-publish"),
        ),
        ("/subject/abi/images/0/reference", json!("rust:latest")),
        ("/resources/memory_mib", json!(0)),
        ("/resources/pids", json!(1025)),
        ("/resources/cpus", json!(33)),
        ("/resources/workload_s", json!(3601)),
        ("/resources/cleanup_s", json!(16)),
        ("/resources/command_s", json!(6)),
    ];
    for (pointer, bad) in cases {
        let mut value = good.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        assert!(parse(&value).is_err(), "{pointer}");
    }
    for field in [
        "verified",
        "authoritative",
        "cid",
        "command",
        "verifier",
        "publisher",
    ] {
        let mut value = good.clone();
        value[field] = json!(true);
        assert!(parse(&value).is_err(), "{field}");
    }
    let mut bad = good.clone();
    bad["resources"]["cleanup_s"] = json!(1);
    assert!(parse(&bad).is_err());
    assert!(requirements::Requirements::parse(&vec![b' '; 32769]).is_err());
    assert!(requirements::Requirements::parse(b"not-json").is_err());
}
#[test]
fn roots_must_be_normal_disjoint_and_outside_candidate() {
    let owner = tempfile::tempdir().unwrap();
    let good = payload(owner.path());
    for path in [
        PathBuf::from("relative"),
        PathBuf::from("/"),
        owner.path().join("missing"),
        owner.path().join("artifact/.."),
        owner.path().join("candidate"),
    ] {
        let mut bad = good.clone();
        bad["artifact_root"] = json!(path);
        let result = parse(&bad).and_then(|item| item.report(&owner.path().join("candidate")));
        assert!(result.is_err());
    }
    let mut overlap = good.clone();
    overlap["lease_root"] = overlap["evidence_root"].clone();
    assert!(parse(&overlap).is_err());
    let nested = owner.path().join("artifact/nested");
    fs::create_dir(&nested).unwrap();
    overlap["lease_root"] = json!(nested);
    assert!(parse(&overlap).is_err());
    let badname = owner.path().join("bad:name");
    fs::create_dir(&badname).unwrap();
    overlap = good.clone();
    overlap["artifact_root"] = json!(badname);
    assert!(parse(&overlap).is_err());
    #[cfg(unix)]
    {
        let alias = owner.path().join("alias");
        std::os::unix::fs::symlink(owner.path().join("artifact"), &alias).unwrap();
        overlap = good;
        overlap["artifact_root"] = json!(alias);
        assert!(parse(&overlap).is_err());
    }
}
