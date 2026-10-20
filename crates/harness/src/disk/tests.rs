use super::precheck::{GIB, check_at, free_bytes};
use crate::ship::git::git;
use std::{cell::RefCell, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn check_with(
    root: &Path,
    floor: Option<&str>,
    ci: bool,
    measure: impl FnMut(&Path) -> Result<u64>,
) -> Result<()> {
    check_at(root, floor, ci, [None, None], measure)
}

fn repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "-b", "test"]).unwrap();
    git(temp.path(), &["config", "user.name", "Disk fixture"]).unwrap();
    git(
        temp.path(),
        &["config", "user.email", "disk@example.invalid"],
    )
    .unwrap();
    git(temp.path(), &["commit", "--allow-empty", "-m", "fixture"]).unwrap();
    temp
}

#[test]
fn either_output_filesystem_below_the_floor_refuses() {
    let temp = repository();
    for low_index in 0..2 {
        let measured = RefCell::new(Vec::new());
        let error = check_with(temp.path(), None, false, |path| {
            let index = measured.borrow().len();
            measured.borrow_mut().push(path.to_owned());
            Ok(if index == low_index {
                14 * GIB
            } else {
                16 * GIB
            })
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("14.00 GiB free"), "{error}");
        assert!(error.contains("15 GiB"), "{error}");
        assert!(error.contains(&measured.borrow()[low_index].display().to_string()));
        assert_eq!(measured.borrow()[0], temp.path().join("target"));
        assert_eq!(measured.borrow()[1], temp.path().join(".git/aoe-cargo"));
    }
    check_with(temp.path(), None, false, |_| Ok(15 * GIB)).unwrap();
    let empty = check_with(temp.path(), Some(""), false, |_| Ok(0))
        .unwrap_err()
        .to_string();
    assert!(empty.contains("floor 15 GiB"), "{empty}");
}

#[test]
fn refusal_lists_only_the_five_largest_worktree_targets_in_size_order() {
    let temp = repository();
    let mut paths = Vec::new();
    for size in [3, 7, 2, 6, 4, 5] {
        let path = temp.path().join(format!("worktree {size}"));
        git(
            temp.path(),
            &["worktree", "add", "--detach", path.to_str().unwrap()],
        )
        .unwrap();
        fs::create_dir(path.join("target")).unwrap();
        fs::write(path.join("target/output"), vec![0; size * 1024]).unwrap();
        paths.push((size, path.join("target")));
    }
    let error = check_with(temp.path(), Some("20"), false, |_| Ok(GIB))
        .unwrap_err()
        .to_string();
    paths.sort_by_key(|(size, _)| std::cmp::Reverse(*size));
    let positions: Vec<_> = paths[..5]
        .iter()
        .map(|(size, path)| {
            let line = format!("{} bytes", size * 1024);
            assert!(error.contains(&line), "{error}");
            error
                .find(&path.display().to_string())
                .expect("target listed")
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{error}"
    );
    assert!(
        !error.contains(&paths[5].1.display().to_string()),
        "{error}"
    );
    assert!(error.contains("20 GiB"), "{error}");
}

#[test]
fn zero_disables_and_ci_skips_without_reading_disk_or_git() {
    let unavailable = Path::new("/no-such-precheck-repository");
    check_with(unavailable, Some("0"), false, |_| panic!("disabled")).unwrap();
    check_with(unavailable, Some("bad"), true, |_| panic!("CI skips")).unwrap();
    assert!(check_with(unavailable, Some("bad"), false, |_| Ok(0)).is_err());
    assert!(check_with(unavailable, Some("18446744073709551615"), false, |_| Ok(0)).is_err());
}

#[test]
fn missing_output_uses_its_existing_parent_filesystem() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing/cache");
    free_bytes(&missing).expect("measure the existing ancestor without creating output");
    assert!(!missing.exists());
}

#[test]
fn custom_storage_is_measured_alongside_the_repository_shared_cache() {
    let temp = repository();
    let target = temp.path().join("custom target");
    let cache = temp.path().join("custom cargo");
    let measured = RefCell::new(Vec::new());
    let error = check_at(
        temp.path(),
        None,
        false,
        [Some(target.clone()), Some(cache.clone())],
        |path| {
            measured.borrow_mut().push(path.to_owned());
            Ok(0)
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(
        *measured.borrow(),
        [
            target.clone(),
            temp.path().join(".git/aoe-cargo"),
            cache.clone()
        ]
    );
    assert!(error.contains(&target.display().to_string()), "{error}");
    assert!(error.contains(&cache.display().to_string()), "{error}");
}
