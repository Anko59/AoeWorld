//! Compare named untouched failures with the original pre-duel source.
use super::focus::Scope;
use crate::ship::git;
use std::{fs, path::Path, process::Command};
use syn::spanned::Spanned;

pub(crate) fn export(root: &Path, revision: &str) -> git::Result<tempfile::TempDir> {
    if !matches!(revision.len(), 40 | 64) || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("baseline needs a full revision".into());
    }
    let cache = root.join(".cache/duel");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let temporary = tempfile::Builder::new()
        .prefix("baseline-")
        .tempdir_in(&cache)
        .map_err(|e| e.to_string())?;
    let archive = cache.join("baseline.tar");
    let output = command(root)
        .args(["archive", "--format=tar", revision])
        .current_dir(root)
        .stdout(fs::File::create(&archive).map_err(|e| e.to_string())?)
        .status()
        .map_err(|e| e.to_string())?;
    if !output.success() {
        return Err("cannot export pre-duel revision".into());
    }
    let status = Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(temporary.path())
        .status()
        .map_err(|e| e.to_string())?;
    fs::remove_file(archive).map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("cannot extract pre-duel revision".into());
    }
    let format = command(root)
        .args(["rev-parse", "--show-object-format"])
        .output()
        .map_err(|e| e.to_string())?;
    if !format.status.success() {
        return Err("cannot identify source Git object format".into());
    }
    let format = String::from_utf8(format.stdout).map_err(|e| e.to_string())?;
    let source = root
        .to_str()
        .ok_or("baseline requires UTF-8 checkout path")?;
    let object_format = format!("--object-format={}", format.trim());
    for args in [
        vec!["init", "--quiet", object_format.as_str()],
        vec!["fetch", "--quiet", "--no-tags", source, revision],
        vec!["update-ref", "--no-deref", "HEAD", revision],
        vec!["reset", "--quiet", "--mixed", revision],
    ] {
        let output = command(temporary.path())
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "baseline Git preparation: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(temporary)
}

fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(key);
        }
    }
    command
}

pub(crate) fn untouched(
    root: &Path,
    base: &str,
    scope: &Scope,
    failures: &[String],
) -> git::Result<Vec<String>> {
    if failures.is_empty() {
        return Ok(Vec::new());
    }
    let mut functions = Vec::new();
    for manifest in &scope.manifests {
        let directory = root
            .join(manifest)
            .parent()
            .ok_or("manifest parent missing")?
            .to_path_buf();
        collect(root, &directory, base, &mut functions)?;
    }
    Ok(failures
        .iter()
        .filter(|name| {
            let function = name
                .rsplit('$')
                .next()
                .unwrap_or(name)
                .split_whitespace()
                .last()
                .unwrap_or(name)
                .rsplit("::")
                .next()
                .unwrap_or(name);
            let matches: Vec<_> = functions.iter().filter(|(n, _)| n == function).collect();
            matches.len() == 1 && matches[0].1
        })
        .cloned()
        .collect())
}
fn collect(
    root: &Path,
    directory: &Path,
    base: &str,
    functions: &mut Vec<(String, bool)>,
) -> git::Result<()> {
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("target" | ".cache" | ".git")
            ) {
                collect(root, &path, base, functions)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy();
            let current = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let old = git::git(root, &["show", &format!("{base}:{relative}")]).ok();
            let Ok(parsed) = syn::parse_file(&current) else {
                continue;
            };
            let old_functions = old
                .as_ref()
                .and_then(|s| syn::parse_file(s).ok())
                .map(|p| bodies(&p.items, old.as_deref().unwrap_or("")))
                .unwrap_or_default();
            for (name, body) in bodies(&parsed.items, &current) {
                let unchanged = old_functions.iter().any(|(n, b)| n == &name && b == &body);
                // Changes outside test functions may change shared helpers or attributes:
                // only exempt a function if all other source bytes remain identical.
                let current_rest = without_tests(&current, &parsed.items);
                let old_rest = old
                    .as_ref()
                    .and_then(|s| syn::parse_file(s).ok().map(|p| without_tests(s, &p.items)));
                functions.push((
                    name,
                    unchanged && old_rest.as_deref() == Some(current_rest.as_str()),
                ));
            }
        }
    }
    Ok(())
}
fn ranges(items: &[syn::Item]) -> Vec<(String, usize, usize)> {
    let mut result = Vec::new();
    for item in items {
        match item {
            syn::Item::Fn(f)
                if f.attrs.iter().any(|a| {
                    a.path()
                        .segments
                        .last()
                        .is_some_and(|s| s.ident.to_string().contains("test"))
                }) =>
            {
                result.push((
                    f.sig.ident.to_string(),
                    f.span().start().line,
                    f.span().end().line,
                ))
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    result.extend(ranges(items));
                }
            }
            _ => (),
        }
    }
    result
}
fn bodies(items: &[syn::Item], source: &str) -> Vec<(String, String)> {
    let lines: Vec<_> = source.lines().collect();
    ranges(items)
        .into_iter()
        .map(|(n, start, end)| (n, lines[start - 1..end].join("\n")))
        .collect()
}
fn without_tests(source: &str, items: &[syn::Item]) -> String {
    let spans = ranges(items);
    source
        .lines()
        .enumerate()
        .filter(|(i, _)| {
            !spans
                .iter()
                .any(|(_, start, end)| i + 1 >= *start && *i < *end)
        })
        .map(|(_, line)| line)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
