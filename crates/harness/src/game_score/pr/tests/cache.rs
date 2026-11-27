use super::*;
use serde_json::json;

fn entry(key: &str) -> Entry {
    Entry {
        key: key.to_owned(),
        revision: "1".repeat(40),
        stored_at: 1_700_000_000,
        workers: vec![1, 15],
        build_seconds: 12.5,
        play_seconds: 61.0,
        report: json!({"schema": 1, "scenarios": []}),
    }
}

#[test]
fn an_entry_comes_back_as_it_went() {
    let directory = tempfile::tempdir().expect("directory");
    let stored = entry(&"a".repeat(64));
    store(directory.path(), &stored).expect("stored");
    assert_eq!(load(directory.path(), &stored.key), Some(stored));
}

#[test]
fn an_unknown_key_a_corrupt_file_and_a_file_of_another_key_are_misses() {
    let directory = tempfile::tempdir().expect("directory");
    assert_eq!(load(directory.path(), "nothing"), None);
    assert_eq!(load(&directory.path().join("absent"), "nothing"), None);
    std::fs::write(directory.path().join("broken.json"), "{ not json").expect("file");
    assert_eq!(load(directory.path(), "broken"), None);
    let wrong = entry("one");
    std::fs::write(
        directory.path().join("other.json"),
        serde_json::to_vec(&wrong).expect("json"),
    )
    .expect("file");
    assert_eq!(
        load(directory.path(), "other"),
        None,
        "the file's key must be the key asked for"
    );
}

#[test]
fn storing_again_replaces_the_entry_and_leaves_no_partial_file() {
    let directory = tempfile::tempdir().expect("directory");
    let mut stored = entry("k");
    store(directory.path(), &stored).expect("first");
    stored.play_seconds = 99.0;
    store(directory.path(), &stored).expect("second");
    assert_eq!(
        load(directory.path(), "k").map(|e| e.play_seconds),
        Some(99.0)
    );
    let names: Vec<String> = std::fs::read_dir(directory.path())
        .expect("listing")
        .flatten()
        .map(|item| item.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["k.json"]);
}

#[test]
fn the_directory_is_made_when_missing() {
    let directory = tempfile::tempdir().expect("directory");
    let nested = directory.path().join("home/results");
    store(&nested, &entry("k")).expect("stored");
    assert!(load(&nested, "k").is_some());
}

fn touch(path: &Path, seconds_since_epoch: u64) {
    let when = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds_since_epoch);
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(when)
        .expect("mtime");
}

#[test]
fn pruning_keeps_the_newest_entries() {
    let directory = tempfile::tempdir().expect("directory");
    for index in 0..5_u64 {
        store(directory.path(), &entry(&format!("k{index}"))).expect("stored");
        touch(
            &directory.path().join(format!("k{index}.json")),
            1_700_000_000 + index * 10,
        );
    }
    prune(directory.path(), 2);
    let kept: Vec<bool> = (0..5)
        .map(|index| load(directory.path(), &format!("k{index}")).is_some())
        .collect();
    assert_eq!(kept, [false, false, false, true, true]);
}

#[test]
fn a_stale_partial_file_goes_and_a_fresh_one_stays() {
    let directory = tempfile::tempdir().expect("directory");
    let stale = directory.path().join("a.1.partial");
    let fresh = directory.path().join("b.2.partial");
    std::fs::write(&stale, "x").expect("stale");
    std::fs::write(&fresh, "x").expect("fresh");
    touch(&stale, 1_700_000_000);
    prune(directory.path(), 10);
    assert!(!stale.exists() && fresh.exists());
}

#[test]
fn a_report_is_kept_without_its_table() {
    let report = json!({"schema": 1, "table": "big", "scenarios": [1]});
    assert_eq!(slim(&report), json!({"schema": 1, "scenarios": [1]}));
}
