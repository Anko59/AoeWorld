//! Load the documented local inputs without running a match or measuring it.
use aoe_map::{GENERATION_RECIPE_VERSION, MAP_SCHEMA_VERSION, MapPackage};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Kind {
    Package,
    Save,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Fixture {
    path: PathBuf,
    kind: Kind,
}

fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || "`\"'()[]<>=".contains(c))
        .filter(|token| !token.is_empty())
        .map(|token| token.trim_end_matches([',', ';', '.']))
}

fn discover(root: &Path) -> Result<Vec<Fixture>, Box<dyn std::error::Error>> {
    let mut fixtures = BTreeSet::new();
    let packages = Path::new(".cache/dev-replay/packages");
    // The docs and workload are the fixture list; do not repeat their hashes or
    // save names here. Historical bare save basenames are deliberately excluded.
    for source in ["docs/scale/save-replay.md", "docs/ai-scaling.md"] {
        let text = fs::read_to_string(root.join(source))?;
        let words: Vec<_> = tokens(&text).collect();
        for word in &words {
            let path = Path::new(word);
            if !path.starts_with(".cache")
                || path
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
            {
                continue;
            }
            let kind = match path.extension().and_then(|ext| ext.to_str()) {
                Some("save") => Kind::Save,
                Some("json") if path.file_stem().and_then(|s| s.to_str()).is_some_and(hash) => {
                    Kind::Package
                }
                _ => continue,
            };
            fixtures.insert(Fixture {
                path: path.into(),
                kind,
            });
        }
        for pair in words.windows(2).filter(|pair| pair[0] == "--map-hash") {
            if !hash(pair[1]) {
                return Err(format!("{source}: invalid --map-hash {}", pair[1]).into());
            }
            fixtures.insert(Fixture {
                path: packages.join(format!("{}.json", pair[1])),
                kind: Kind::Package,
            });
        }
    }
    let workload: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("qa/ai-scaling.json"))?)?;
    let map_hash = workload["map_hash"]
        .as_str()
        .filter(|value| hash(value))
        .ok_or("qa/ai-scaling.json: invalid map_hash")?;
    fixtures.insert(Fixture {
        path: packages.join(format!("{map_hash}.json")),
        kind: Kind::Package,
    });
    Ok(fixtures.into_iter().collect())
}

fn load(root: &Path, fixture: &Fixture) -> Result<(), String> {
    let path = root.join(&fixture.path);
    match fixture.kind {
        Kind::Package => {
            let package: MapPackage =
                serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            package.validate().map_err(|e| e.to_string())?;
            if path.file_stem().and_then(|s| s.to_str())
                != Some(package.content_hash_hex().as_str())
            {
                return Err("package content hash does not match filename".into());
            }
            Ok(())
        }
        Kind::Save => {
            let packages = path
                .parent()
                .and_then(Path::parent)
                .ok_or("save has no package directory")?;
            // This is the game's decoder and restore, including resource-overlay
            // validation. No AI, socket, clock, tick or replay is started.
            aoe_server::GameplayService::from_saved_replay(&path, packages).map(|_| ())
        }
    }
}

fn versions(kind: Kind) -> String {
    let package = format!(
        "crates/map/src/lib.rs: MAP_SCHEMA_VERSION={MAP_SCHEMA_VERSION}, GENERATION_RECIPE_VERSION={GENERATION_RECIPE_VERSION}"
    );
    match kind {
        Kind::Package => package,
        Kind::Save => format!(
            "crates/server/src/gameplay_match/saves.rs: SAVE_VERSION; crates/simulation/src/game_world/snapshot/records.rs: WORLD_SNAPSHOT_VERSION; {package}"
        ),
    }
}

/// Return false only for present inputs that fail loading; keep checking the rest.
fn check(root: &Path, fixtures: &[Fixture], output: &mut impl Write) -> std::io::Result<bool> {
    let mut passed = true;
    for fixture in fixtures {
        let path = fixture.path.display();
        match fs::metadata(root.join(&fixture.path)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                writeln!(output, "SKIPPED {path} (missing)")?;
                continue;
            }
            _ => (), // Permission errors and non-files must fail through the loader.
        }
        match load(root, fixture) {
            Ok(()) => writeln!(output, "OK {path}")?,
            Err(error) => {
                passed = false;
                writeln!(output, "FAIL {path}: {error} [{}]", versions(fixture.kind))?;
            }
        }
    }
    Ok(passed)
}

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(".");
    let fixtures = discover(root)?;
    if !check(root, &fixtures, &mut std::io::stdout().lock())? {
        return Err("present documented fixtures failed to load".into());
    }
    Ok(())
}
