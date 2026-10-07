//! Where a path really lands: `~` expanded, `.`/`..` folded, and symlinks
//! followed (dangling ones too) so a link cannot smuggle a write elsewhere.
use std::{
    collections::VecDeque,
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_HOPS: usize = 16;

pub(crate) fn expand_home(text: &str, home: Option<&Path>) -> PathBuf {
    match (text.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home.to_path_buf(),
        (Some(rest), Some(home)) if rest.starts_with('/') => home.join(&rest[1..]),
        _ => PathBuf::from(text),
    }
}

/// Fold `.` and `..` without touching the file system.
pub(crate) fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    out
}

/// The absolute location an absolute `path` resolves to, or `None` when it
/// takes more than 16 symlink hops.
pub(crate) fn land(path: &Path) -> Option<PathBuf> {
    let mut pending: VecDeque<_> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let mut out = PathBuf::from("/");
    let mut hops = 0;
    while let Some(part) = pending.pop_front() {
        let part = PathBuf::from(part);
        match part.components().next() {
            Some(Component::RootDir) => out = PathBuf::from("/"),
            Some(Component::ParentDir) => {
                out.pop();
            }
            Some(Component::Normal(name)) => {
                let candidate = out.join(name);
                let link = fs::symlink_metadata(&candidate)
                    .ok()
                    .filter(|metadata| metadata.file_type().is_symlink())
                    .and_then(|_| fs::read_link(&candidate).ok());
                match link {
                    Some(target) => {
                        hops += 1;
                        if hops > MAX_HOPS {
                            return None;
                        }
                        if target.is_absolute() {
                            out = PathBuf::from("/");
                        }
                        for component in target.components().rev() {
                            pending.push_front(component.as_os_str().to_owned());
                        }
                    }
                    None => out = candidate,
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// `path` relative to `base`, compared case-insensitively so `.CACHE/Claude-Hook`
/// on a case-insensitive file system cannot slip past a guard.
pub(crate) fn relative_to(path: &Path, base: &Path) -> Option<String> {
    let path = path.to_str()?.to_ascii_lowercase();
    let base = base.to_str()?.to_ascii_lowercase();
    if path == base {
        return Some(String::new());
    }
    path.strip_prefix(&format!("{}/", base.trim_end_matches('/')))
        .map(str::to_owned)
}

pub(crate) fn within(path: &Path, base: &Path) -> bool {
    relative_to(path, base).is_some()
}
