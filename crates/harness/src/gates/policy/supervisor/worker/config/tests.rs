use super::*;
use serde_json::json;
pub(crate) fn bytes() -> Vec<u8> {
    serde_json::to_vec(&json!({"schema":1,"image":format!("sha256:{}","2".repeat(64)),"uid":65532,
        "memory_mib":128,"pids":32,"cpus":1,"workload_s":20,"cleanup_s":10,"command_s":1,"tmp_mib":16,"target_mib":64})).unwrap()
}
#[test]
fn closed_template_rejects_duplicate_unknown_missing_and_authority_fields() {
    assert!(Config::parse(&bytes()).is_ok());
    let text = String::from_utf8(bytes()).unwrap();
    let duplicate = text.replacen('{', "{\"schema\":1,", 1);
    assert!(Config::parse(duplicate.as_bytes()).is_err());
    let mut value: serde_json::Value = serde_json::from_slice(&bytes()).unwrap();
    for name in [
        "argv",
        "endpoint",
        "role",
        "authoritative",
        "service_uid",
        "paths",
    ] {
        value[name] = json!(true);
        assert!(Config::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        value.as_object_mut().unwrap().remove(name);
    }
    value.as_object_mut().unwrap().remove("uid");
    assert!(Config::parse(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn all_template_resource_and_immutable_image_bounds_are_enforced() {
    let base: serde_json::Value = serde_json::from_slice(&bytes()).unwrap();
    for (field, invalid) in [
        ("schema", 2),
        ("uid", 0),
        ("memory_mib", 63),
        ("memory_mib", 32769),
        ("pids", 0),
        ("pids", 1025),
        ("cpus", 0),
        ("cpus", 33),
        ("workload_s", 0),
        ("workload_s", 3601),
        ("cleanup_s", 0),
        ("cleanup_s", 16),
        ("command_s", 0),
        ("command_s", 6),
        ("tmp_mib", 15),
        ("tmp_mib", 1025),
        ("target_mib", 63),
        ("target_mib", 8193),
    ] {
        let mut value = base.clone();
        value[field] = json!(invalid);
        assert!(
            Config::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{field}:{invalid}"
        );
    }
    for image in [
        "alpine:latest",
        "sha256:1234",
        "sha256:ZZZZ",
        "--privileged",
    ] {
        let mut value = base.clone();
        value["image"] = json!(image);
        assert!(Config::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value = base;
    value["cleanup_s"] = json!(1);
    value["command_s"] = json!(2);
    assert!(Config::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(Config::parse(&vec![b' '; 32769]).is_err());
}
