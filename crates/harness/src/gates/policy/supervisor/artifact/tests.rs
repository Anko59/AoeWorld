use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn signer() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}
fn subject() -> Value {
    json!({"schema":1,"repository_id":7,"protected_ref":"refs/heads/dev","protected_commit":"1".repeat(40),"protected_tree":"2".repeat(40),"closure_blake3":"3".repeat(64),"registry_hash":format!("blake3:registry-v2-canonical-v1:{}","4".repeat(64)),"executable_blake3":"5".repeat(64),"runtime_blake3":"6".repeat(64),"trust_root_id":key_id(&signer().verifying_key().to_bytes()),"abi":{"schema":1,"abi":1,"registry_schema":2,"images":[{"reference":"fixture-manifest","actual_id":"fixture-config"}],"dispatch":["fixture-closed-operations"]}})
}
fn envelope(bytes: &[u8]) -> Envelope {
    Envelope {
        schema: 1,
        key_id: key_id(&signer().verifying_key().to_bytes()),
        signature: encode(&signer().sign(&message(bytes)).to_bytes()),
    }
}
fn observed() -> key::Observed {
    let public_key = signer().verifying_key().to_bytes();
    key::Observed {
        repository_id: 7,
        key_id: key_id(&public_key),
        public_key,
        file_blake3: "8".repeat(64),
    }
}
fn check(bytes: &[u8], envelope: &Envelope) -> Value {
    evaluate(
        bytes,
        7,
        &key_id(&signer().verifying_key().to_bytes()),
        Some(envelope),
        || Ok(observed()),
    )
}
#[test]
fn strict_signature_observation_never_admits_service() {
    let bytes = serde_json::to_vec(&subject()).unwrap();
    let envelope = envelope(&bytes);
    let report = check(&bytes, &envelope);
    assert_eq!(report["status"], "VERIFIED_SIGNATURE_NON_AUTHORITATIVE");
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["admission_granted"], false);
    assert_eq!(
        report["subject_blake3"],
        blake3::hash(&bytes).to_hex().to_string()
    );
    assert_eq!(report["key_id"], envelope.key_id);
    assert!(report["limits"].to_string().contains("not independent"));
}
#[test]
fn tampering_every_bound_dimension_or_raw_encoding_rejects_signature() {
    let good = subject();
    let bytes = serde_json::to_vec(&good).unwrap();
    let proof = envelope(&bytes);
    for field in [
        "schema",
        "repository_id",
        "protected_ref",
        "protected_commit",
        "protected_tree",
        "closure_blake3",
        "registry_hash",
        "executable_blake3",
        "runtime_blake3",
        "trust_root_id",
    ] {
        let mut bad = good.clone();
        bad[field] = json!("tampered");
        assert_eq!(
            check(&serde_json::to_vec(&bad).unwrap(), &proof)["status"],
            "REJECTED",
            "{field}"
        );
    }
    for field in ["schema", "abi", "registry_schema", "images", "dispatch"] {
        let mut bad = good.clone();
        bad["abi"][field] = json!("tampered");
        assert_eq!(
            check(&serde_json::to_vec(&bad).unwrap(), &proof)["status"],
            "REJECTED",
            "abi.{field}"
        );
    }
    let mut raw = bytes.clone();
    raw.push(b' ');
    assert_eq!(check(&raw, &proof)["status"], "REJECTED");
    let mut wrong_domain = proof;
    wrong_domain.signature = encode(&signer().sign(&bytes).to_bytes());
    assert_eq!(check(&bytes, &wrong_domain)["status"], "REJECTED");
}
#[test]
fn absence_and_malformed_envelopes_never_consult_key() {
    assert_eq!(
        evaluate(b"", 7, "", None, || panic!("absent envelope queried key"))["status"],
        "ABSENT"
    );
    let bytes = serde_json::to_vec(&subject()).unwrap();
    let good = envelope(&bytes);
    for bad in [
        Envelope {
            schema: 2,
            ..envelope(&bytes)
        },
        Envelope {
            signature: "a".repeat(127),
            ..envelope(&bytes)
        },
        Envelope {
            signature: "A".repeat(128),
            ..envelope(&bytes)
        },
        Envelope {
            signature: "a".repeat(129),
            ..envelope(&bytes)
        },
        Envelope {
            signature: format!("{}\n", "a".repeat(128)),
            ..envelope(&bytes)
        },
        Envelope {
            key_id: "x".repeat(64),
            ..envelope(&bytes)
        },
    ] {
        assert_eq!(
            evaluate(&bytes, 7, &good.key_id, Some(&bad), || panic!(
                "malformed envelope queried key"
            ))["status"],
            "REJECTED"
        );
    }
    assert_eq!(
        evaluate(&vec![b'a'; 16385], 7, &good.key_id, Some(&good), || panic!(
            "oversized subject queried key"
        ))["status"],
        "REJECTED"
    );
    assert_eq!(
        evaluate(&bytes, 7, &"0".repeat(64), Some(&good), || panic!(
            "different subject root queried key"
        ))["status"],
        "REJECTED"
    );
    assert!(serde_json::from_value::<Envelope>(json!({"schema":1,"key_id":good.key_id,"signature":good.signature,"public_key":"injected"})).is_err());
    assert!(serde_json::from_slice::<Envelope>(b"{bad").is_err());
}
#[test]
fn key_repository_self_identity_signature_and_weak_key_fail_closed() {
    let bytes = serde_json::to_vec(&subject()).unwrap();
    let proof = envelope(&bytes);
    assert_eq!(
        evaluate(&bytes, 8, &proof.key_id, Some(&proof), || Ok(observed()))["status"],
        "REJECTED"
    );
    assert_eq!(
        evaluate(&bytes, 7, &proof.key_id, Some(&proof), || {
            let mut k = observed();
            k.key_id = "0".repeat(64);
            Ok(k)
        })["status"],
        "REJECTED"
    );
    let mut bad = envelope(&bytes);
    bad.signature = "0".repeat(128);
    assert_eq!(check(&bytes, &bad)["status"], "REJECTED");
    let mut weak = [0u8; 32];
    weak[0] = 1;
    let weak_id = key_id(&weak);
    let weak_proof = Envelope {
        schema: 1,
        key_id: weak_id.clone(),
        signature: "0".repeat(128),
    };
    assert_eq!(
        evaluate(&bytes, 7, &weak_id, Some(&weak_proof), || Ok(
            key::Observed {
                repository_id: 7,
                public_key: weak,
                key_id: weak_id.clone(),
                file_blake3: "x".into()
            }
        ))["status"],
        "REJECTED"
    );
    assert_eq!(
        evaluate(&bytes, 7, &proof.key_id, Some(&proof), || Err(
            "fixed root-key unavailable"
        ))["status"],
        "UNAVAILABLE_KEY"
    );
}
#[cfg(unix)]
fn key_payload() -> Value {
    let public = signer().verifying_key().to_bytes();
    json!({"schema":1,"repository_id":7,"public_key":encode(&public),"key_id":key_id(&public)})
}
#[cfg(unix)]
fn write_key(path: &std::path::Path, value: &Value) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}
#[cfg(unix)]
#[test]
fn private_file_observation_and_real_signature_positive_do_not_qualify_operator() {
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("key.json");
    write_key(&path, &key_payload());
    let uid = nix::unistd::geteuid().as_raw();
    let bytes = serde_json::to_vec(&subject()).unwrap();
    let proof = envelope(&bytes);
    let report = evaluate(&bytes, 7, &proof.key_id, Some(&proof), || {
        key::read(&path, uid, owner.path())
    });
    assert_eq!(report["status"], "VERIFIED_SIGNATURE_NON_AUTHORITATIVE");
    assert_eq!(report["admission_granted"], false);
    assert_eq!(
        report["key_file_blake3"],
        blake3::hash(&std::fs::read(&path).unwrap())
            .to_hex()
            .to_string()
    );
    assert!(key::read(&path, uid.wrapping_add(1), owner.path()).is_err());
    assert!(key::read(&owner.path().join("missing"), uid, owner.path()).is_err());
}
#[cfg(unix)]
#[test]
fn key_json_and_size_are_strict_and_no_unsigned_fallback_exists() {
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("key.json");
    let uid = nix::unistd::geteuid().as_raw();
    let good = key_payload();
    for (field, bad) in [
        ("schema", json!(2)),
        ("repository_id", json!(0)),
        ("public_key", json!("A".repeat(64))),
        ("public_key", json!("1".repeat(63))),
        ("key_id", json!("0".repeat(64))),
    ] {
        let mut value = good.clone();
        value[field] = bad;
        write_key(&path, &value);
        assert!(key::read(&path, uid, owner.path()).is_err(), "{field}");
    }
    let mut unknown = good;
    unknown["verified"] = json!(true);
    write_key(&path, &unknown);
    assert!(key::read(&path, uid, owner.path()).is_err());
    std::fs::write(&path, vec![b' '; 4097]).unwrap();
    assert!(key::read(&path, uid, owner.path()).is_err());
    std::fs::write(&path, b"not-json").unwrap();
    assert!(key::read(&path, uid, owner.path()).is_err());
}
#[cfg(unix)]
#[test]
fn leaf_and_ancestor_links_aliases_permissions_and_wrong_boundary_reject() {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("key.json");
    let uid = nix::unistd::geteuid().as_raw();
    write_key(&path, &key_payload());
    for mode in [0o640, 0o644, 0o660, 0o400, 0o1600] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert!(key::read(&path, uid, owner.path()).is_err());
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = owner.path().join("link");
    symlink(&path, &link).unwrap();
    assert!(key::read(&link, uid, owner.path()).is_err());
    let hard = owner.path().join("hard");
    fs::hard_link(&path, &hard).unwrap();
    assert!(key::read(&path, uid, owner.path()).is_err());
    let nested = owner.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
    write_key(&nested.join("key.json"), &key_payload());
    let alias = owner.path().join("alias");
    symlink(&nested, &alias).unwrap();
    assert!(key::read(&alias.join("key.json"), uid, owner.path()).is_err());
    for mode in [0o720, 0o702] {
        fs::set_permissions(&nested, fs::Permissions::from_mode(mode)).unwrap();
        assert!(key::read(&nested.join("key.json"), uid, owner.path()).is_err());
    }
    assert!(key::read(std::path::Path::new("relative"), uid, owner.path()).is_err());
    let other = tempfile::tempdir().unwrap();
    assert!(key::read(&path, uid, other.path()).is_err());
    fs::set_permissions(owner.path(), fs::Permissions::from_mode(0o777)).unwrap();
    assert!(key::read(&path, uid, owner.path()).is_err());
}
