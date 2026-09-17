use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("git listing failed: {0}")]
    Git(String),
    #[error("cannot inspect {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("repository structure violations:\n{0}")]
    Violations(String),
}

fn tracked(root: &Path) -> Result<Vec<PathBuf>, PolicyError> {
    let result = Command::new("git")
        .args(["ls-files", "--cached", "-z"])
        .current_dir(root)
        .output()
        .map_err(|e| PolicyError::Git(e.to_string()))?;
    if !result.status.success() {
        return Err(PolicyError::Git(
            String::from_utf8_lossy(&result.stderr).to_string(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(result
            .stdout
            .split(|b| *b == 0)
            .filter(|b| !b.is_empty())
            .map(|b| PathBuf::from(OsString::from_vec(b.to_vec())))
            .collect())
    }
    #[cfg(not(unix))]
    {
        Ok(result
            .stdout
            .split(|b| *b == 0)
            .filter(|b| !b.is_empty())
            .map(|b| PathBuf::from(String::from_utf8_lossy(b).to_string()))
            .collect())
    }
}

fn is_text(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|x| x.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "Makefile"
                    | "Dockerfile"
                    | "AGENTS.md"
                    | ".gitignore"
                    | ".dockerignore"
                    | ".prettierignore"
            ) || name.ends_with(".Dockerfile")
        })
    {
        return true;
    }
    let Some(ext) = path.extension().and_then(|x| x.to_str()) else {
        return false;
    };
    matches!(
        ext,
        "rs" | "toml"
            | "md"
            | "yaml"
            | "yml"
            | "json"
            | "ts"
            | "tsx"
            | "js"
            | "mjs"
            | "html"
            | "css"
            | "svg"
            | "wgsl"
            | "sh"
            | "txt"
    )
}

fn is_code_or_config(path: &Path) -> bool {
    path.file_name().and_then(|x| x.to_str()).is_some_and(|x| {
        matches!(
            x,
            "Makefile" | "Dockerfile" | ".gitignore" | ".dockerignore" | ".prettierignore"
        ) || x.ends_with(".Dockerfile")
    }) || path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
        matches!(
            x,
            "rs" | "toml" | "yaml" | "yml" | "json" | "ts" | "tsx" | "js" | "mjs" | "sh" | "wgsl"
        )
    })
}

pub fn structure(root: &Path) -> Result<(), PolicyError> {
    let mut count = BTreeMap::<PathBuf, usize>::new();
    let mut problems = Vec::new();
    for relative in tracked(root)? {
        let path = root.join(&relative);
        if !path.is_file() {
            continue;
        }
        if is_code_or_config(&relative) {
            *count
                .entry(
                    relative
                        .parent()
                        .unwrap_or_else(|| Path::new(""))
                        .to_path_buf(),
                )
                .or_default() += 1;
        }
        if is_text(&relative) {
            if relative
                .file_name()
                .is_some_and(|x| x == "Cargo.lock" || x == "package-lock.json")
            {
                continue;
            }
            let content = fs::read(&path).map_err(|source| PolicyError::Io { path, source })?;
            if std::str::from_utf8(&content).is_err() || content.contains(&0) {
                problems.push(format!("{}: expected UTF-8 text", relative.display()));
                continue;
            }
            let lines = content.iter().filter(|b| **b == b'\n').count()
                + usize::from(!content.is_empty() && !content.ends_with(b"\n"));
            if lines > 500 {
                problems.push(format!("{}: {lines} lines (max 500)", relative.display()));
            }
        }
    }
    for (directory, total) in count {
        if total > 14 {
            problems.push(format!(
                "{}: {total} code/config files (max 14)",
                directory.display()
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(PolicyError::Violations(problems.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn recognized_text_formats_include_shaders_and_svg() {
        assert!(is_text(Path::new("test.wgsl")));
        assert!(is_text(Path::new("test.svg")));
        assert!(!is_text(Path::new("image.png")));
        assert!(is_text(Path::new("docker/runtime.Dockerfile")));
        assert!(is_text(Path::new(".dockerignore")));
        assert!(is_code_or_config(Path::new("docker/runtime.Dockerfile")));
    }

    #[test]
    fn unusual_tracked_names_and_limits_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        git(root, &["init", "-q"]);
        fs::write(root.join("snow ☃ space.wgsl"), "line\n".repeat(501)).unwrap();
        fs::create_dir(root.join("many")).unwrap();
        for index in 0..15 {
            fs::write(root.join(format!("many/{index}.rs")), "pub fn f() {}\n").unwrap();
        }
        git(root, &["add", "-A"]);
        let result = structure(root).unwrap_err().to_string();
        assert!(result.contains("snow ☃ space.wgsl"));
        assert!(result.contains("15 code/config files"));
        fs::remove_file(root.join("snow ☃ space.wgsl")).unwrap();
        git(root, &["add", "-A"]);
        let result = structure(root).unwrap_err().to_string();
        assert!(!result.contains("snow ☃ space.wgsl"));
    }
}
