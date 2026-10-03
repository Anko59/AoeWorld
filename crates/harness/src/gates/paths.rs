//! Lossless path selection: disabling rename folding includes both rename sides.
#[cfg(test)]
use std::collections::BTreeSet;
use std::{error::Error, path::Path, process::Command};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
    }
    Ok(output.stdout)
}

fn names(bytes: Vec<u8>) -> Result<Vec<String>> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8(part.to_vec()).map_err(Into::into))
        .collect()
}

pub(super) fn resolve(root: &Path, reference: &str) -> Result<String> {
    let reference = if let Some(tail) = reference.strip_prefix("origin/") {
        format!("refs/remotes/origin/{tail}")
    } else {
        reference.to_owned()
    };
    Ok(String::from_utf8(git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )?)?
    .trim()
    .to_owned())
}

pub(super) fn changed(root: &Path, base: &str) -> Result<Vec<String>> {
    let base = resolve(root, base)?;
    let merge_base = String::from_utf8(git(root, &["merge-base", &base, "HEAD"])?)?;
    names(git(
        root,
        &[
            "diff",
            "--no-renames",
            "--name-only",
            "-z",
            merge_base.trim(),
            "HEAD",
            "--",
        ],
    )?)
}

#[cfg(test)]
pub(super) fn working(root: &Path) -> Result<Vec<String>> {
    let mut paths: BTreeSet<_> = names(git(
        root,
        &["diff", "--no-renames", "--name-only", "-z", "HEAD", "--"],
    )?)?
    .into_iter()
    .collect();
    paths.extend(names(git(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?)?);
    Ok(paths.into_iter().collect())
}

#[cfg(test)]
pub(super) fn staged(root: &Path) -> Result<Vec<String>> {
    names(git(
        root,
        &[
            "diff",
            "--cached",
            "--no-renames",
            "--name-only",
            "-z",
            "--",
        ],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn command(root: &Path, args: &[&str]) {
        git(root, args).expect("git fixture");
    }
    fn repo() -> tempfile::TempDir {
        let temp = tempfile::tempdir().expect("repository");
        command(temp.path(), &["init", "-q"]);
        command(
            temp.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        command(temp.path(), &["config", "user.name", "Fixture"]);
        command(temp.path(), &["config", "commit.gpgsign", "false"]);
        fs::write(temp.path().join("code.rs"), "pub fn original() {}\n").expect("source");
        command(temp.path(), &["add", "."]);
        command(temp.path(), &["commit", "-qm", "base"]);
        temp
    }

    #[test]
    fn rename_deletion_and_unusual_names_are_retained() {
        let temp = repo();
        let root = temp.path();
        let base = resolve(root, "HEAD").expect("base");
        fs::rename(root.join("code.rs"), root.join("guide space.md")).expect("rename");
        command(root, &["add", "-A"]);
        assert_eq!(staged(root).expect("index"), ["code.rs", "guide space.md"]);
        command(root, &["commit", "-qm", "rename"]);
        assert_eq!(
            changed(root, &base).expect("paths"),
            ["code.rs", "guide space.md"]
        );
        fs::remove_file(root.join("guide space.md")).expect("delete");
        fs::write(root.join("new ☃.rs"), "new").expect("untracked");
        assert_eq!(
            working(root).expect("paths"),
            ["guide space.md", "new ☃.rs"]
        );
    }

    #[test]
    fn origin_reference_cannot_be_shadowed_by_local_tag() {
        let temp = repo();
        let root = temp.path();
        let base = resolve(root, "HEAD").expect("base");
        command(root, &["update-ref", "refs/remotes/origin/dev", &base]);
        fs::write(root.join("extra.rs"), "extra").expect("new source");
        command(root, &["add", "."]);
        command(root, &["commit", "-qm", "new"]);
        command(root, &["tag", "origin/dev"]);
        assert_eq!(resolve(root, "origin/dev").expect("remote"), base);
        assert_eq!(changed(root, "origin/dev").expect("paths"), ["extra.rs"]);
        assert!(resolve(root, "missing-ref").is_err());
    }
}
