//! COMPARE_* settings, read once from the environment (`make compare` exports
//! them) and checked before anything is built or started.
use super::server::running_url;
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

pub(crate) const DEFAULT_BOOKMARKS: &str = "qa/bookmarks.json";
/// A bookmarks file is data; anything larger is a mistake.
pub(crate) const BOOKMARKS_LIMIT: u64 = 256 * 1024;
const DEFAULT_VIEW_SECONDS: u64 = 90;
const DEFAULT_BUDGET_MINUTES: u64 = 40;

/// Where one side's app comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// Build it (the merge base for `before`, the working tree for `after`)
    /// and serve it on a free port of this run's own.
    Build,
    /// An app that is already running.
    Url(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) bookmarks: PathBuf,
    pub(crate) views: Option<String>,
    pub(crate) before: Source,
    pub(crate) after: Source,
    pub(crate) before_packages: PathBuf,
    pub(crate) after_packages: PathBuf,
    pub(crate) base: Option<String>,
    pub(crate) asset_pack: Option<String>,
    pub(crate) out: Option<PathBuf>,
    pub(crate) view_seconds: u64,
    pub(crate) budget_minutes: u64,
}

fn source(name: &str, value: Option<String>) -> Result<Source> {
    match value.as_deref() {
        None | Some("" | "build") => Ok(Source::Build),
        Some(url) => running_url(name, url)
            .map(Source::Url)
            .map_err(|e| e.to_string()),
    }
}

fn number(name: &str, value: Option<String>, default: u64, range: (u64, u64)) -> Result<u64> {
    let Some(text) = value.filter(|v| !v.is_empty()) else {
        return Ok(default);
    };
    match text.parse::<u64>() {
        Ok(n) if (range.0..=range.1).contains(&n) => Ok(n),
        _ => Err(format!(
            "{name}={text}: use a whole number from {} to {}",
            range.0, range.1
        )),
    }
}

/// A path below `.cache/` of the checkout, with no `..`: the only place a
/// comparison writes screenshots.
pub(crate) fn output_directory(root: &Path, requested: &Path) -> Result<PathBuf> {
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let cache = root.join(".cache");
    if path.components().any(|c| c == Component::ParentDir)
        || !path.starts_with(&cache)
        || path == cache
    {
        return Err(format!(
            "COMPARE_OUT={}: use a directory below .cache/",
            requested.display()
        ));
    }
    Ok(path)
}

impl Config {
    pub(crate) fn from_env(root: &Path, env: &dyn Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |name: &str| env(name).filter(|v| !v.is_empty());
        let packages = |name: &str| {
            get(name).map_or_else(|| root.join("local-assets/map-packages"), |p| root.join(p))
        };
        Ok(Self {
            bookmarks: root
                .join(get("COMPARE_BOOKMARKS").unwrap_or_else(|| DEFAULT_BOOKMARKS.into())),
            views: get("COMPARE_VIEWS"),
            before: source("COMPARE_BEFORE", env("COMPARE_BEFORE"))?,
            after: source("COMPARE_AFTER", env("COMPARE_AFTER"))?,
            before_packages: packages("COMPARE_BEFORE_PACKAGES"),
            after_packages: packages("COMPARE_AFTER_PACKAGES"),
            base: get("COMPARE_BASE"),
            asset_pack: get("COMPARE_ASSET_PACK"),
            out: get("COMPARE_OUT").map(PathBuf::from),
            view_seconds: number(
                "COMPARE_VIEW_TIMEOUT_SECONDS",
                env("COMPARE_VIEW_TIMEOUT_SECONDS"),
                DEFAULT_VIEW_SECONDS,
                (10, 600),
            )?,
            budget_minutes: number(
                "COMPARE_BUDGET_MINUTES",
                env("COMPARE_BUDGET_MINUTES"),
                DEFAULT_BUDGET_MINUTES,
                (1, 240),
            )?,
        })
    }
}

/// The local asset pack the games run with: COMPARE_ASSET_PACK (a directory
/// name below `local-assets/packs`), else the only pack there. Without one the
/// game does not start, so the browser would wait for nothing.
pub(crate) fn asset_pack(root: &Path, requested: Option<&str>) -> Result<String> {
    let packs = root.join("local-assets/packs");
    let usable = |name: &str| {
        !name.is_empty()
            && !name.contains(['/', '\\'])
            && !name.starts_with('.')
            && packs.join(name).join("manifest.json").is_file()
    };
    if let Some(name) = requested {
        return usable(name).then(|| name.to_owned()).ok_or_else(|| {
            format!(
                "COMPARE_ASSET_PACK={name}: no such pack with a manifest in {}",
                packs.display()
            )
        });
    }
    let mut found: Vec<String> = std::fs::read_dir(&packs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| usable(name))
        .collect();
    found.sort();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(format!(
            "no local asset pack in {}: the game needs one (`make assets-import`, docs/assets.md)",
            packs.display()
        )),
        _ => Err(format!(
            "several packs in {}: choose one with COMPARE_ASSET_PACK=<name>",
            packs.display()
        )),
    }
}

/// The default output directory name for a branch.
pub(crate) fn branch_directory(branch: &str) -> String {
    let name: String = branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(['.', '-']).to_owned();
    if name.is_empty() {
        "detached".into()
    } else {
        name
    }
}
