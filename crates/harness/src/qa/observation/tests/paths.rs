use super::*;
#[test]
fn relative_wire_paths_resolve_from_current_directory_not_evidence_root() {
    let expected = std::env::current_dir()
        .unwrap()
        .join("reports/qa/screen.bin");
    assert_eq!(
        io::absolute(Path::new("reports/qa/screen.bin")).unwrap(),
        expected
    );
    assert!(io::absolute(Path::new("reports/./qa/screen.bin")).is_err());
    assert!(io::absolute(Path::new("reports/../qa/screen.bin")).is_err());
}
#[test]
fn report_self_directory_missing_escape_and_non_normal_paths_reject() {
    let (owner, root, input, artifact, mut report) = fixture();
    rejected(&root, &input, &mut report, &input);
    rejected(&root, &input, &mut report, &root);
    rejected(&root, &input, &mut report, &root.join("missing"));
    let outside = owner.path().join("outside.bin");
    fs::write(&outside, b"outside").unwrap();
    rejected(&root, &input, &mut report, &outside);
    rejected(&root, &input, &mut report, &root.join("../qa/screen.bin"));
    rejected(&root, &input, &mut report, &root.join("./screen.bin"));
    report.journeys[0].evidence = vec![artifact.to_str().unwrap().into()];
    save(&input, &report);
    assert!(observe_file_at(&input, &root).is_ok());
    assert!(observe_file_at(&input, &root.join("../qa")).is_err());
    let normal_alias = format!("{}//screen.bin", root.display());
    report.journeys[0].evidence.push(normal_alias);
    save(&input, &report);
    assert!(observe_file_at(&input, &root).is_err());
}
#[cfg(unix)]
#[test]
fn symlink_ancestors_leaf_dangling_hardlinks_and_fifo_never_become_evidence() {
    use std::os::unix::fs::symlink;
    let (owner, root, input, artifact, mut report) = fixture();
    let leaf = root.join("linked.bin");
    symlink(&artifact, &leaf).unwrap();
    rejected(&root, &input, &mut report, &leaf);
    let dangling = root.join("dangling.bin");
    symlink(root.join("absent"), &dangling).unwrap();
    rejected(&root, &input, &mut report, &dangling);
    let inside_alias = root.join("directory-link");
    symlink(&root, &inside_alias).unwrap();
    rejected(&root, &input, &mut report, &inside_alias.join("screen.bin"));
    let alias = owner.path().join("alias");
    symlink(&root, &alias).unwrap();
    rejected(&root, &input, &mut report, &alias.join("screen.bin"));
    report.journeys[0].evidence = vec![artifact.to_str().unwrap().into()];
    save(&input, &report);
    assert!(observe_file_at(&alias.join("session.json"), &alias).is_err());
    let fifo = root.join("fifo");
    nix::unistd::mkfifo(
        &fifo,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    rejected(&root, &input, &mut report, &fifo);
    let hard = root.join("hard.bin");
    fs::hard_link(&artifact, &hard).unwrap();
    rejected(&root, &input, &mut report, &hard);
    rejected(&root, &input, &mut report, &artifact);
    let hard_report = root.join("report-hard.json");
    fs::hard_link(&input, &hard_report).unwrap();
    assert!(observe_file_at(&input, &root).is_err());
}
#[test]
fn byte_limits_aggregate_count_and_held_endpoint_growth_fail_closed() {
    let (_owner, root, input, artifact, mut report) = fixture();
    let file = fs::OpenOptions::new().write(true).open(&artifact).unwrap();
    file.set_len(ARTIFACT_BYTES + 1).unwrap();
    assert!(observe_file_at(&input, &root).is_err());
    file.set_len(3).unwrap();
    let mut held = Held::open(&artifact, ARTIFACT_BYTES).unwrap();
    let hash = held.digest().unwrap();
    file.set_len(4).unwrap();
    assert!(held.recheck(&hash).is_err());
    let mut held = Held::open(&artifact, ARTIFACT_BYTES).unwrap();
    let hash = held.digest().unwrap();
    fs::write(&artifact, b"abcd").unwrap();
    assert!(held.recheck(&hash).is_err());
    for n in 0..5 {
        let path = root.join(format!("large{n}"));
        fs::File::create(&path)
            .unwrap()
            .set_len(ARTIFACT_BYTES)
            .unwrap();
        report.journeys[n].evidence = vec![path.to_str().unwrap().into()];
    }
    save(&input, &report);
    assert!(observe_file_at(&input, &root).is_err());
    let mut names = vec![];
    for n in 0..129 {
        let path = root.join(format!("empty{n}"));
        fs::write(&path, []).unwrap();
        names.push(path.to_str().unwrap().to_string());
    }
    report.status = Status::Blocked;
    report.journeys.clear();
    for (n, chunk) in names.chunks(64).enumerate() {
        report.journeys.push(Journey {
            name: format!("bounded{n}"),
            completed: false,
            evidence: chunk.to_vec(),
        });
    }
    save(&input, &report);
    assert!(observe_file_at(&input, &root).is_err());
    fs::OpenOptions::new()
        .write(true)
        .open(&input)
        .unwrap()
        .set_len(REPORT_BYTES as u64 + 1)
        .unwrap();
    assert!(observe_file_at(&input, &root).is_err());
}
