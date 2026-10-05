use super::super::io::{Pair, absolute, write_known};
use super::*;

#[test]
fn original_pair_fd_bytes_hashes_and_late_rewrite_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(temp.path(), &wire, &inventory);
    let output = temp.path().join(OUTPUT).join("mutants.out");
    let mut pair = Pair::open(&output).unwrap();
    assert_eq!(
        pair.measurements.outcomes.bytes,
        pair.outcomes_bytes.len() as u64
    );
    assert_eq!(
        pair.measurements.inventory.raw_blake3,
        blake3::hash(&pair.inventory_bytes).to_hex().to_string()
    );
    assert_eq!(
        pair.measurements.outcomes.raw_blake3,
        blake3::hash(&pair.outcomes_bytes).to_hex().to_string()
    );
    pair.recheck().unwrap();
    fs::write(
        output.join("outcomes.json"),
        b"changed while original descriptor is held",
    )
    .unwrap();
    assert!(pair.recheck().is_err());
    let mut pair = Pair::open(&output).unwrap();
    fs::write(output.join("mutants.json"), b"[]").unwrap();
    assert!(pair.recheck().is_err());
}
#[test]
fn missing_directory_and_oversized_artifacts_cannot_be_measured() {
    assert_eq!(
        absolute(Path::new(".")).unwrap(),
        std::env::current_dir().unwrap()
    );
    let temp = tempfile::tempdir().unwrap();
    assert!(Pair::open(temp.path()).is_err());
    fs::create_dir(temp.path().join("outcomes.json")).unwrap();
    fs::write(temp.path().join("mutants.json"), b"[]").unwrap();
    assert!(Pair::open(temp.path()).is_err());
    let other = tempfile::tempdir().unwrap();
    fs::write(other.path().join("mutants.json"), b"[]").unwrap();
    let file = fs::File::create(other.path().join("outcomes.json")).unwrap();
    file.set_len(4 * 1024 * 1024 + 1).unwrap();
    assert!(Pair::open(other.path()).is_err());
    assert!(Pair::open(&other.path().join("../elsewhere")).is_err());
}
#[cfg(unix)]
#[test]
fn symlink_hardlink_fifo_and_symlink_ancestors_are_rejected_before_read() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("mutants.json"), b"[]").unwrap();
    fs::write(temp.path().join("original"), b"{}").unwrap();
    symlink(
        temp.path().join("original"),
        temp.path().join("outcomes.json"),
    )
    .unwrap();
    assert!(Pair::open(temp.path()).is_err());
    let linked = tempfile::tempdir().unwrap();
    fs::write(linked.path().join("outcomes.json"), b"{}").unwrap();
    fs::hard_link(
        linked.path().join("outcomes.json"),
        linked.path().join("mutants.json"),
    )
    .unwrap();
    assert!(Pair::open(linked.path()).is_err());
    let fifo = tempfile::tempdir().unwrap();
    fs::write(fifo.path().join("mutants.json"), b"[]").unwrap();
    nix::unistd::mkfifo(
        &fifo.path().join("outcomes.json"),
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    assert!(Pair::open(fifo.path()).is_err());
    let root = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(root.path(), &wire, &inventory);
    let alias = tempfile::tempdir().unwrap();
    symlink(
        root.path().join(OUTPUT).join("mutants.out"),
        alias.path().join("linked"),
    )
    .unwrap();
    assert!(Pair::open(&alias.path().join("linked")).is_err());
    assert!(Pair::open(Path::new("/dev/null")).is_err());
}
#[cfg(unix)]
#[test]
fn held_directory_mode_correlation_and_original_inode_replacement_are_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(root.path(), &wire, &inventory);
    let output = root.path().join(OUTPUT).join("mutants.out");
    let mut pair = Pair::open(&output).unwrap();
    let mode = fs::metadata(&output).unwrap().permissions().mode();
    fs::set_permissions(&output, fs::Permissions::from_mode(mode ^ 0o010)).unwrap();
    assert!(pair.recheck().is_err());
    fs::set_permissions(&output, fs::Permissions::from_mode(mode)).unwrap();
    let path = output.join("outcomes.json");
    let replaced = output.join("old-outcomes");
    // The exact owned regular path and destination are verified before this move.
    assert_eq!(path.canonicalize().unwrap(), path);
    assert!(replaced.starts_with(root.path()));
    assert!(!replaced.exists());
    fs::rename(&path, &replaced).unwrap();
    fs::write(&path, &pair.outcomes_bytes).unwrap();
    assert!(pair.recheck().is_err());
}
#[cfg(unix)]
#[test]
fn publication_never_follows_symlink_or_truncates_a_hardlink_target() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("reports/mutation")).unwrap();
    let sentinel = root.path().join("sentinel");
    fs::write(&sentinel, b"must stay unchanged").unwrap();
    symlink(&sentinel, root.path().join("reports/mutation/nightly.json")).unwrap();
    assert!(write_known(root.path(), "nightly.json", b"replacement").is_err());
    assert_eq!(fs::read(&sentinel).unwrap(), b"must stay unchanged");
    let linked = tempfile::tempdir().unwrap();
    fs::create_dir_all(linked.path().join("reports/mutation")).unwrap();
    let sentinel = linked.path().join("sentinel");
    fs::write(&sentinel, b"unchanged hardlink").unwrap();
    fs::hard_link(
        &sentinel,
        linked.path().join("reports/mutation/nightly.json"),
    )
    .unwrap();
    assert!(write_known(linked.path(), "nightly.json", b"replacement").is_err());
    assert_eq!(fs::read(&sentinel).unwrap(), b"unchanged hardlink");
    assert!(write_known(linked.path(), "unknown-name", b"replacement").is_err());
}
#[test]
fn pending_publication_precedes_any_artifact_failure_and_safe_limits_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    invalidate(root.path()).unwrap();
    let json: Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(json["verdict"], "INCONCLUSIVE");
    assert_eq!(json["authoritative"], false);
    let report = write_report(root.path(), &Ok(())).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    assert!(report.artifacts.is_none());
    assert_eq!(report.artifact_freshness, "UNAVAILABLE");
    assert_eq!(report.execution_binding, "UNAVAILABLE");
}
