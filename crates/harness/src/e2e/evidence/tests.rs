use super::{combine, join};
use serde_json::{Value, json};
use std::path::Path;

fn body(shard: &str, revision: &str, result: &str) -> Value {
    json!({
        "version": 1, "shard": shard, "revision": revision,
        "server_binary": "target/release/aoe-server",
        "asset_source": "generated CI fixtures", "asset_pack": "",
        "landscape_fixture": {"content_hash": "abc"}, "result": result
    })
}

fn write(directory: &Path, index: u32, count: u32, value: &Value) {
    let path = directory.join(format!("shard-{index}-of-{count}.json"));
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn all(directory: &Path, count: u32) {
    for index in 1..=count {
        write(
            directory,
            index,
            count,
            &body(&format!("{index}/{count}"), "r1", "PASS"),
        );
    }
}

fn refusal(directory: &Path) -> String {
    join(directory).unwrap_err().to_string()
}

#[test]
fn every_passing_shard_of_one_revision_writes_the_release_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    all(temporary.path(), 4);
    combine(temporary.path()).unwrap();
    let report: Value =
        serde_json::from_slice(&std::fs::read(temporary.path().join("pass.json")).unwrap())
            .unwrap();
    assert_eq!(report, {
        let mut expected = body("", "r1", "PASS");
        expected.as_object_mut().unwrap().remove("shard");
        expected
    });
}

#[test]
fn a_missing_shard_is_refused() {
    let temporary = tempfile::tempdir().unwrap();
    all(temporary.path(), 4);
    std::fs::remove_file(temporary.path().join("shard-3-of-4.json")).unwrap();
    assert!(refusal(temporary.path()).contains("shard 3/4 is missing"));
    std::fs::remove_file(temporary.path().join("shard-4-of-4.json")).unwrap();
    assert!(refusal(temporary.path()).contains("is missing"));
    assert!(combine(temporary.path()).is_err());
    assert!(!temporary.path().join("pass.json").exists());
}

#[test]
fn no_shards_at_all_is_refused() {
    let temporary = tempfile::tempdir().unwrap();
    assert!(refusal(temporary.path()).contains("no shard evidence"));
}

#[test]
fn shards_of_two_revisions_are_refused() {
    let temporary = tempfile::tempdir().unwrap();
    all(temporary.path(), 3);
    write(temporary.path(), 2, 3, &body("2/3", "r2", "PASS"));
    assert!(refusal(temporary.path()).contains("differs"));
}

#[test]
fn a_shard_that_did_not_pass_is_refused() {
    let temporary = tempfile::tempdir().unwrap();
    all(temporary.path(), 3);
    write(temporary.path(), 2, 3, &body("2/3", "r1", "FAIL"));
    assert!(refusal(temporary.path()).contains("shard 2/3 did not pass"));
}

#[test]
fn mixed_shard_counts_and_misnamed_files_are_refused() {
    let temporary = tempfile::tempdir().unwrap();
    all(temporary.path(), 3);
    write(temporary.path(), 1, 4, &body("1/4", "r1", "PASS"));
    assert!(refusal(temporary.path()).contains("split differently"));
    std::fs::remove_file(temporary.path().join("shard-1-of-4.json")).unwrap();
    write(temporary.path(), 2, 3, &body("1/3", "r1", "PASS"));
    assert!(refusal(temporary.path()).contains("different shard"));
}
