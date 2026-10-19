use crate::agents::prune::built as prune;
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
    assert_eq!(prune(&home, &hash(1)).unwrap(), 2);
    assert!(current.join("aoe-harness").is_file(), "the judge in use");
    assert!(newest.is_dir(), "the newest other judge");
    assert!(!old.exists() && !oldest.exists());
}

#[test]
fn removes_young_judges_beyond_the_two_after_a_build() {
    let (_temp, home) = home();
    judge(&home, &hash(1), 5 * HOUR);
    let young = judge(&home, &hash(2), Duration::from_secs(300));
    let younger = judge(&home, &hash(3), Duration::from_secs(30));
    let old = judge(&home, &hash(4), 4 * HOUR);
    let older = judge(&home, &hash(5), 6 * HOUR);
    assert_eq!(prune(&home, &hash(1)).unwrap(), 3);
    assert!(!young.exists(), "only two completed judges survive a build");
    assert!(younger.is_dir(), "the newest other judge");
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
    assert_eq!(prune(&home, &hash(1)).unwrap(), 1);
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
    for keep in ["", "..", "/", &hash(9), "bootstrap-", "target"] {
        assert_eq!(prune(&home, keep).unwrap(), 0, "keep {keep:?}");
    }
    // A keep naming a directory without a binary is not "the judge in use".
    fs::create_dir(home.join(hash(8))).expect("directory");
    assert_eq!(prune(&home, &hash(8)).unwrap(), 0);
    // A directory not named aoe-agent-hook, an empty path, a missing one.
    let elsewhere = temp.path().join("elsewhere");
    judge(&elsewhere, &hash(1), 5 * HOUR);
    judge(&elsewhere, &hash(2), 6 * HOUR);
    judge(&elsewhere, &hash(3), 7 * HOUR);
    assert_eq!(prune(&elsewhere, &hash(1)).unwrap(), 0);
    assert_eq!(prune(Path::new(""), &hash(1)).unwrap(), 0);
    assert_eq!(prune(Path::new("/"), &hash(1)).unwrap(), 0);
    assert_eq!(
        prune(&temp.path().join("missing/aoe-agent-hook"), &hash(1)).unwrap(),
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
    assert_eq!(prune(&home, &hash(1)).unwrap(), 2);
    assert_eq!(prune(&home, &hash(1)).unwrap(), 0);
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
    assert_eq!(prune(&home, &hash(1)).unwrap(), 0);
    assert!(target.join("aoe-harness").is_file());
}

#[test]
fn disk_hygiene_build_pruning_keeps_exactly_two_even_when_all_are_recent() {
    let (_temp, home) = home();
    let keep = hash(1);
    let current = judge(&home, &keep, Duration::ZERO);
    let newest_other = judge(&home, &hash(2), Duration::from_secs(1));
    let old = judge(&home, &hash(3), Duration::from_secs(2));
    assert_eq!(crate::agents::prune::built(&home, &keep).unwrap(), 1);
    assert!(current.is_dir() && newest_other.is_dir());
    assert!(!old.exists());
}

#[test]
fn disk_hygiene_removes_incremental_before_testing_the_target_bound() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let incremental = target.join("debug/incremental");
    fs::create_dir_all(&incremental).unwrap();
    fs::write(incremental.join("large"), [0; 50]).unwrap();
    fs::write(target.join("debug/dependency"), [0; 10]).unwrap();
    crate::agents::prune::clean_target_with(&target, 20, &mut || {
        panic!("incremental removal brought the target under the bound")
    })
    .unwrap();
    assert!(!incremental.exists());
    assert!(target.join("debug/dependency").exists());
}

#[test]
fn disk_hygiene_cargo_clean_is_required_only_over_the_remaining_bound() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("release/incremental")).unwrap();
    fs::write(target.join("dependency"), [0; 21]).unwrap();
    let mut cleaned = false;
    crate::agents::prune::clean_target_with(&target, 20, &mut || {
        assert!(!target.join("release/incremental").exists());
        cleaned = true;
        fs::remove_dir_all(&target)
    })
    .unwrap();
    assert!(cleaned);
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("dependency"), [0; 20]).unwrap();
    crate::agents::prune::clean_target_with(&target, 20, &mut || panic!("at the bound")).unwrap();
}

#[test]
fn disk_hygiene_clean_failure_and_an_ineffective_clean_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("dependency"), [0; 21]).unwrap();
    let error = crate::agents::prune::clean_target_with(&target, 20, &mut || {
        Err(std::io::Error::other("clean failed"))
    })
    .unwrap_err();
    assert!(error.to_string().contains("clean failed"));
    assert!(crate::agents::prune::clean_target_with(&target, 20, &mut || Ok(())).is_err());
}
