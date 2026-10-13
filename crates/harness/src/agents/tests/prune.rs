use crate::agents::prune::{YOUNG, prune};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(20)
}

/// A judge directory whose binary was built `age` ago.
fn judge(home: &Path, name: &str, age: Duration) -> PathBuf {
    let dir = home.join(name);
    fs::create_dir_all(&dir).expect("judge directory");
    let file = fs::File::create(dir.join("aoe-harness")).expect("binary");
    file.set_modified(SystemTime::now() - age).expect("mtime");
    dir
}

fn home() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let home = temp.path().join("aoe-agent-hook");
    fs::create_dir(&home).expect("home");
    (temp, home)
}

const HOUR: Duration = Duration::from_secs(3600);

#[test]
fn keeps_the_judge_in_use_and_the_newest_other_and_removes_older() {
    let (_temp, home) = home();
    let current = judge(&home, &hash(1), 5 * HOUR);
    let newest = judge(&home, &format!("bootstrap-{}", hash(2)), 2 * HOUR);
    let old = judge(&home, &hash(3), 3 * HOUR);
    let oldest = judge(&home, &format!("bootstrap-{}", hash(4)), 9 * HOUR);
    assert_eq!(prune(&home, &hash(1), SystemTime::now()), 2);
    assert!(current.join("aoe-harness").is_file(), "the judge in use");
    assert!(newest.is_dir(), "the newest other judge");
    assert!(!old.exists() && !oldest.exists());
}

#[test]
fn keeps_young_judges_besides_the_two() {
    let (_temp, home) = home();
    judge(&home, &hash(1), 5 * HOUR);
    let young = judge(&home, &hash(2), YOUNG / 2);
    let younger = judge(&home, &hash(3), Duration::from_secs(30));
    let old = judge(&home, &hash(4), 4 * HOUR);
    let older = judge(&home, &hash(5), 6 * HOUR);
    assert_eq!(prune(&home, &hash(1), SystemTime::now()), 2);
    assert!(
        young.is_dir() && younger.is_dir(),
        "younger than ten minutes"
    );
    assert!(!old.exists() && !older.exists());
}

#[test]
fn leaves_unrelated_entries_alone() {
    let (_temp, home) = home();
    judge(&home, &hash(1), HOUR);
    judge(&home, &hash(2), 8 * HOUR);
    judge(&home, &hash(3), 9 * HOUR);
    let other = judge(&home, "target", 9 * HOUR);
    let short = judge(&home, "abc123", 9 * HOUR);
    let upper = judge(&home, &hash(0xab).to_uppercase(), 9 * HOUR);
    fs::create_dir_all(home.join("source/crates")).expect("source");
    fs::write(home.join("build.lock"), "").expect("lock");
    fs::write(home.join(hash(5)), "a file named like a judge").expect("file");
    let empty = home.join(format!("bootstrap-{}", hash(6)));
    fs::create_dir(&empty).expect("directory without a binary");
    assert_eq!(prune(&home, &hash(1), SystemTime::now()), 1);
    for kept in [&other, &short, &upper, &empty, &home.join("source")] {
        assert!(kept.exists(), "{kept:?}");
    }
    assert!(home.join("build.lock").is_file() && home.join(hash(5)).is_file());
}

#[test]
fn wrong_home_or_keep_deletes_nothing() {
    let (temp, home) = home();
    judge(&home, &hash(1), 5 * HOUR);
    judge(&home, &hash(2), 6 * HOUR);
    judge(&home, &hash(3), 7 * HOUR);
    let now = SystemTime::now();
    for keep in ["", "..", "/", &hash(9), "bootstrap-", "target"] {
        assert_eq!(prune(&home, keep, now), 0, "keep {keep:?}");
    }
    // A keep naming a directory without a binary is not "the judge in use".
    fs::create_dir(home.join(hash(8))).expect("directory");
    assert_eq!(prune(&home, &hash(8), now), 0);
    // A directory not named aoe-agent-hook, an empty path, a missing one.
    let elsewhere = temp.path().join("elsewhere");
    judge(&elsewhere, &hash(1), 5 * HOUR);
    judge(&elsewhere, &hash(2), 6 * HOUR);
    judge(&elsewhere, &hash(3), 7 * HOUR);
    assert_eq!(prune(&elsewhere, &hash(1), now), 0);
    assert_eq!(prune(Path::new(""), &hash(1), now), 0);
    assert_eq!(prune(Path::new("/"), &hash(1), now), 0);
    assert_eq!(
        prune(&temp.path().join("missing/aoe-agent-hook"), &hash(1), now),
        0
    );
    for n in [1, 2, 3] {
        assert!(home.join(hash(n)).is_dir() && elsewhere.join(hash(n)).is_dir());
    }
}

#[test]
fn a_second_pruner_finds_nothing_left_to_do() {
    let (_temp, home) = home();
    judge(&home, &hash(1), 5 * HOUR);
    judge(&home, &hash(2), 6 * HOUR);
    judge(&home, &hash(3), 7 * HOUR);
    judge(&home, &hash(4), 8 * HOUR);
    let now = SystemTime::now();
    assert_eq!(prune(&home, &hash(1), now), 2);
    assert_eq!(prune(&home, &hash(1), now), 0);
    assert!(home.join(hash(2)).is_dir());
}

#[cfg(unix)]
#[test]
fn a_symlink_named_like_a_judge_is_not_followed() {
    let (temp, home) = home();
    judge(&home, &hash(1), 5 * HOUR);
    judge(&home, &hash(2), 6 * HOUR);
    let target = judge(temp.path(), "precious", 9 * HOUR);
    std::os::unix::fs::symlink(&target, home.join(hash(3))).expect("symlink");
    assert_eq!(prune(&home, &hash(1), SystemTime::now()), 0);
    assert!(target.join("aoe-harness").is_file());
}
