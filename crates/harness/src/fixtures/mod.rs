//! Load the documented local inputs without running a match or measuring it.
use aoe_map::{GENERATION_RECIPE_VERSION, MAP_SCHEMA_VERSION};
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
        // The game's map store check: manifest, canonical name, pages and
        // navigation data, exactly as the server loads a package directory.
        Kind::Package => aoe_server::verify_package(&path).map_err(|e| e.to_string()),
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

/// What a failure means for the version constants: "expected X, found Y" for each
/// package constant that differs, or a statement that the versions matched, so the
/// format or content changed without a version bump.
fn versions(root: &Path, fixture: &Fixture, error: &str) -> String {
    match fixture.kind {
        Kind::Package => {
            let recorded: serde_json::Value = fs::read(root.join(&fixture.path))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let moved: Vec<String> = [
                ("schema_version", "MAP_SCHEMA_VERSION", MAP_SCHEMA_VERSION),
                (
                    "generation_recipe_version",
                    "GENERATION_RECIPE_VERSION",
                    GENERATION_RECIPE_VERSION,
                ),
            ]
            .into_iter()
            .filter_map(|(field, constant, current)| {
                let found = recorded.get(field)?.as_u64()?;
                (found != u64::from(current)).then(|| {
                    format!("crates/map/src/lib.rs {constant}: expected {current}, found {found}")
                })
            })
            .collect();
            if moved.is_empty() {
                format!(
                    "MAP_SCHEMA_VERSION={MAP_SCHEMA_VERSION} and GENERATION_RECIPE_VERSION={GENERATION_RECIPE_VERSION} match: the package content no longer validates"
                )
            } else {
                moved.join("; ")
            }
        }
        Kind::Save if error.contains("version") => format!(
            "the save's version differs from SAVE_VERSION={save} or WORLD_SNAPSHOT_VERSION={snapshot}: re-record the save",
            save = aoe_server::SAVE_VERSION,
            snapshot = aoe_simulation::WORLD_SNAPSHOT_VERSION
        ),
        Kind::Save => format!(
            "SAVE_VERSION={save} and WORLD_SNAPSHOT_VERSION={snapshot} match the save, so the save format or the game content it refers to changed without a version bump: re-record the save",
            save = aoe_server::SAVE_VERSION,
            snapshot = aoe_simulation::WORLD_SNAPSHOT_VERSION
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
                writeln!(
                    output,
                    "FAIL {path}: {error} [{}]",
                    versions(root, fixture, &error)
                )?;
            }
        }
    }
    Ok(passed)
}

/// `report` (the ship's preflight form) warns instead of failing: a fixture an
/// earlier merge broke must not block every later, unrelated ship. The warning is
/// the gate's last line, so it is the summary the evidence and reviewers see.
pub(crate) fn run(report: bool) -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(".");
    let fixtures = discover(root)?;
    let passed = check(root, &fixtures, &mut std::io::stdout().lock())?;
    // The last stderr line is the gate's summary in the ship evidence.
    let message = "documented fixtures failed to load; regenerate them (docs/scale/save-replay.md)";
    match (passed, report) {
        (true, _) => eprintln!(
            "fixtures: {} documented, all present ones load",
            fixtures.len()
        ),
        (false, true) => eprintln!("WARNING: {message}"),
        (false, false) => return Err(message.into()),
    }
    Ok(())
}
