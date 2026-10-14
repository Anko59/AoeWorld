//! The floor of real checkouts: untracked entries that are not plain files
//! (symlinks, nested repositories), raw paths and the uncommitted count.
use super::super::{Change, Levels, Part};
use crate::ship::{
    describe::Level,
    tests::{fixture, run},
};
use std::{fs, os::unix::fs::symlink, path::Path};

fn working(root: &Path) -> Change {
    let levels = Levels::load(root).unwrap();
    Change::collect_working(root, &levels, "dev", false).unwrap()
}

fn part<'a>(change: &'a Change, path: &str) -> Option<&'a Part> {
    change.parts.iter().find(|part| part.path == path)
}

#[test]
fn an_untracked_symlink_to_a_directory_counts_as_one_line() {
    let (_temp, root) = fixture("true");
    symlink(root.join("gates"), root.join("local-assets")).unwrap();
    let change = working(&root);
    assert_eq!(part(&change, "local-assets").expect("counted").lines, 1);
    assert_eq!(change.uncommitted, 1);
}

#[test]
fn a_dangling_symlink_counts_as_one_line() {
    let (_temp, root) = fixture("true");
    symlink("/nonexistent/aoe-target", root.join("dangling")).unwrap();
    let change = working(&root);
    assert_eq!(part(&change, "dangling").expect("counted").lines, 1);
    assert_eq!(change.uncommitted, 1);
}

#[test]
fn a_nested_repository_is_left_out_and_the_floor_is_still_computed() {
    let (_temp, root) = fixture("true");
    let nested = root.join("nested");
    fs::create_dir(&nested).unwrap();
    run(&nested, &["init", "-q"]);
    fs::write(nested.join("a.rs"), "1\n2\n").unwrap();
    let change = working(&root);
    assert!(
        change
            .parts
            .iter()
            .all(|part| !part.path.starts_with("nested")),
        "{:?}",
        change.parts
    );
    // The committed README change (two lines) is still the change; nothing
    // uncommitted was counted.
    assert_eq!(change.lines, 2);
    assert_eq!(change.uncommitted, 0);
}

#[test]
fn a_path_git_would_quote_is_classified_by_its_real_name() {
    let (_temp, root) = fixture("true");
    // Without `-z` Git prints this name quoted and octal-escaped, and it
    // would match no suite and no path prefix.
    let name = "gates/é \"q\".txt";
    fs::write(root.join(name), "x\n".repeat(250)).unwrap();
    let levels = Levels::load(&root).unwrap();
    let change = working(&root);
    let counted = part(&change, name).expect("the raw path");
    assert_eq!(
        (counted.lines, counted.suites.contains("everything")),
        (250, true)
    );
    assert_eq!(levels.floor(&change).level, Level::High);
    assert_eq!(change.uncommitted, 1);
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "quoted"]);
    let committed = Change::collect(&root, &levels, "dev", false).unwrap();
    assert_eq!(part(&committed, name).expect("raw path").lines, 250);
    assert_eq!(levels.floor(&committed).level, Level::High);
    assert_eq!(committed.uncommitted, 0);
}

#[test]
fn a_leading_space_is_part_of_the_first_listed_path() {
    let (_temp, root) = fixture("true");
    fs::write(root.join(" lead.rs"), "a\nb\n").unwrap();
    let change = working(&root);
    assert_eq!(part(&change, " lead.rs").expect("raw path").lines, 2);
}

#[test]
fn an_untracked_directory_of_twenty_files_counts_each_file() {
    let (_temp, root) = fixture("true");
    fs::create_dir_all(root.join("web/new")).unwrap();
    for n in 0..20 {
        fs::write(root.join(format!("web/new/f{n}.ts")), "a\nb\nc\n").unwrap();
    }
    let change = working(&root);
    let counted: Vec<&Part> = change
        .parts
        .iter()
        .filter(|part| part.path.starts_with("web/new/"))
        .collect();
    assert_eq!(counted.len(), 20);
    assert!(counted.iter().all(|part| part.lines == 3));
    assert_eq!(change.uncommitted, 20);
}

#[test]
fn a_staged_rename_is_two_uncommitted_files() {
    let (_temp, root) = fixture("true");
    run(&root, &["mv", "README.md", "GUIDE.md"]);
    let change = working(&root);
    assert!(part(&change, "README.md").is_some(), "{:?}", change.parts);
    assert!(part(&change, "GUIDE.md").is_some(), "{:?}", change.parts);
    assert_eq!(change.uncommitted, 2);
}
