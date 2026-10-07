//! Edit cadence: right after Claude edits a file, check that file and its
//! directory against the structure rules (UTF-8, 500 lines, 14 code/config
//! files per directory) so the limit is met while the change is small.
use super::{context::Context, paths, shell::Word};
use std::{fs, path::Path, process::Command};

const MAX_LINES: usize = 500;
const MAX_CODE_FILES: usize = 14;

pub(crate) fn findings(context: &Context, cwd: Option<&Path>, file: &str) -> Vec<String> {
    let Ok(Some(landing)) = context.resolve(cwd, &Word::literal(file)) else {
        return Vec::new();
    };
    let Ok(relative) = landing.strip_prefix(&context.root) else {
        return Vec::new();
    };
    if paths::relative_to(&landing, &context.root.join(".cache")).is_some() {
        return Vec::new();
    }
    let mut problems = Vec::new();
    if crate::policy::is_text(relative) {
        match fs::read(&landing) {
            Ok(content) if std::str::from_utf8(&content).is_err() || content.contains(&0) => {
                problems.push(format!("{}: expected UTF-8 text", relative.display()));
            }
            Ok(content) => {
                let lines = content.iter().filter(|b| **b == b'\n').count()
                    + usize::from(!content.is_empty() && !content.ends_with(b"\n"));
                if lines > MAX_LINES {
                    problems.push(format!(
                        "{}: {lines} lines (max {MAX_LINES}); split it into focused modules",
                        relative.display()
                    ));
                }
            }
            Err(_) => {}
        }
    }
    if crate::policy::is_code_or_config(relative) {
        let directory = relative.parent().unwrap_or(Path::new(""));
        let count = sibling_code_files(&context.root, directory);
        if count > MAX_CODE_FILES {
            problems.push(format!(
                "{}/: {count} code/config files (max {MAX_CODE_FILES}); move modules into a subdirectory",
                directory.display()
            ));
        }
    }
    problems
}

/// Tracked and new (not ignored) code/config files directly in `directory`.
fn sibling_code_files(root: &Path, directory: &Path) -> usize {
    let mut pathspec = directory.as_os_str().to_owned();
    if !pathspec.is_empty() {
        pathspec.push("/");
    }
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
        ])
        .arg(if pathspec.is_empty() {
            ".".into()
        } else {
            pathspec
        })
        .output()
    else {
        return 0;
    };
    let mut names: Vec<_> = output
        .stdout
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| Path::new(std::str::from_utf8(name).unwrap_or_default()).to_path_buf())
        .filter(|path| path.parent().unwrap_or(Path::new("")) == directory)
        .filter(|path| root.join(path).is_file())
        .filter(|path| crate::policy::is_code_or_config(path))
        .collect();
    names.sort();
    names.dedup();
    names.len()
}
