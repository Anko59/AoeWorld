//! `make game-map`: make sure the owner's pinned map package exists locally, then print its hash
//! and directory. Workers, agents and CI call it; the request is the committed
//! `qa/anatolia-map.json`. See `docs/geodata/licences.md` for why packages never leave a cache.
//!
//! A package already published for the request is reused as it is. Otherwise the geodata worker
//! fills the source cache (`bootstrap`, with retries), generates the package and verifies it.
//! `--no-fetch` never runs the worker: a missing package reports "map unavailable" (cached sources
//! would not help, the generator reads the providers' catalogs online). `--soft` turns that
//! report into exit 0 with a marker file, so a pull request does not fail for a network reason and
//! the later job can show the line.
use aoe_map::{MapPackage, MapRequest};
use clap::Args;
use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Written next to the packages when the map could not be made; removed when it is there.
pub(crate) const UNAVAILABLE_MARKER: &str = "UNAVAILABLE";
const REQUEST_LIMIT: usize = 64 * 1024;
const FAILURE_TAIL_LINES: usize = 12;

#[derive(Args, Debug, Clone)]
pub(crate) struct Options {
    /// The map request (a JSON `MapRequest`).
    #[arg(long, env = "AOE_MAP_REQUEST", default_value = "qa/anatolia-map.json")]
    pub(crate) request: PathBuf,
    /// The geodata source cache.
    #[arg(long, env = "AOE_GEODATA_CACHE", default_value = ".cache/geodata")]
    pub(crate) cache: PathBuf,
    /// The directory that holds package manifests and their `pages/<hash>/` directories.
    #[arg(
        long,
        env = "AOE_MAP_PACKAGE",
        default_value = ".cache/game-map/packages"
    )]
    pub(crate) packages: PathBuf,
    /// The release `aoe-map-worker` executable.
    #[arg(long, default_value = "target/release/aoe-map-worker")]
    pub(crate) worker: PathBuf,
    /// Tries for each download step.
    #[arg(long, default_value_t = 3)]
    pub(crate) attempts: u32,
    /// Seconds to wait between tries.
    #[arg(long, default_value_t = 30)]
    pub(crate) retry_wait_seconds: u64,
    /// Never download or generate: use the package, else report "map unavailable".
    #[arg(long)]
    pub(crate) no_fetch: bool,
    /// Report "map unavailable" and exit 0 (with a marker file) instead of failing.
    #[arg(long)]
    pub(crate) soft: bool,
}

#[derive(Debug)]
pub(crate) struct Ready {
    pub(crate) request_id: String,
    pub(crate) hash: String,
    pub(crate) manifest: PathBuf,
    pub(crate) generated: bool,
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Ready(Ready),
    Unavailable(String),
}

pub(crate) fn run(options: Options) -> Result<()> {
    let outcome = ensure(&options)?;
    for line in report(&options, outcome)? {
        println!("{line}");
    }
    Ok(())
}

fn read_limited(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() > REQUEST_LIMIT {
        return Err(format!("{} exceeds {REQUEST_LIMIT} bytes", path.display()).into());
    }
    Ok(bytes)
}

pub(crate) fn parse_request(bytes: &[u8]) -> Result<MapRequest> {
    let request: MapRequest = serde_json::from_slice(bytes)?;
    Ok(request.normalized()?)
}

/// `game-map-<16 hex>`: the normalized request, so equal requests share a key. It names the map in
/// the report; the CI cache key is built from the request file and the generator's code
/// (`.github/workflows/game-map.yml`), because a package is only the map of the code that made it.
pub(crate) fn cache_key(request: &MapRequest) -> String {
    let canonical = serde_json::to_vec(request).unwrap_or_default();
    let digest = blake3::hash(&canonical).to_hex();
    format!("game-map-{}", &digest.as_str()[..16])
}

/// The package published in `directory` for `request`, if its manifest and pages are there.
fn find_existing(directory: &Path, request: &MapRequest) -> Result<Option<Ready>> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(None);
    };
    let mut manifests: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    manifests.sort();
    for manifest in manifests {
        // A manifest the current reader refuses is an old package: it is not ours.
        let Ok(bytes) = fs::read(&manifest) else {
            continue;
        };
        let Ok(package) = serde_json::from_slice::<MapPackage>(&bytes) else {
            continue;
        };
        let hash = package.content_hash_hex();
        let named = manifest.file_stem().and_then(|stem| stem.to_str()) == Some(hash.as_str());
        if named
            && package.request == *request
            && package.validate().is_ok()
            && directory.join("pages").join(&hash).is_dir()
        {
            return Ok(Some(Ready {
                request_id: cache_key(request),
                hash,
                manifest,
                generated: false,
            }));
        }
    }
    Ok(None)
}

pub(crate) fn ensure(options: &Options) -> Result<Outcome> {
    let request = parse_request(&read_limited(&options.request)?)?;
    if let Some(ready) = find_existing(&options.packages, &request)? {
        return Ok(Outcome::Ready(ready));
    }
    if options.no_fetch {
        return Ok(Outcome::Unavailable(format!(
            "map unavailable: {} holds no package for {} and --no-fetch forbids making one",
            options.packages.display(),
            cache_key(&request)
        )));
    }
    let worker = Worker { options };
    if let Err(error) = worker.retrying("bootstrap") {
        return Ok(Outcome::Unavailable(format!("map unavailable: {error}")));
    }
    fs::create_dir_all(&options.packages)?;
    if let Err(error) = worker.retrying("map-generate") {
        return Ok(Outcome::Unavailable(format!("map unavailable: {error}")));
    }
    let Some(mut ready) = find_existing(&options.packages, &request)? else {
        return Err(format!(
            "map-generate succeeded but {} holds no package for {}",
            options.packages.display(),
            cache_key(&request)
        )
        .into());
    };
    worker.verify(&ready.manifest)?;
    ready.generated = true;
    Ok(Outcome::Ready(ready))
}

/// The lines to print; in soft mode an unavailable map is a line and a marker, not an error.
pub(crate) fn report(options: &Options, outcome: Outcome) -> Result<Vec<String>> {
    let marker = options.packages.join(UNAVAILABLE_MARKER);
    match outcome {
        Outcome::Ready(ready) => {
            let _ = fs::remove_file(&marker);
            let directory = options
                .packages
                .canonicalize()
                .unwrap_or_else(|_| options.packages.clone());
            Ok(vec![
                format!("hash={}", ready.hash),
                format!("request={}", ready.request_id),
                format!("directory={}", directory.display()),
                format!(
                    "manifest={}",
                    directory.join(format!("{}.json", ready.hash)).display()
                ),
                format!(
                    "source={}",
                    if ready.generated {
                        "generated"
                    } else {
                        "existing"
                    }
                ),
            ])
        }
        Outcome::Unavailable(reason) if options.soft => {
            fs::create_dir_all(&options.packages)?;
            fs::write(&marker, format!("{reason}\n"))?;
            Ok(vec![reason])
        }
        Outcome::Unavailable(reason) => Err(reason.into()),
    }
}

struct Worker<'a> {
    options: &'a Options,
}

impl Worker<'_> {
    fn run(&self, arguments: &[&str]) -> std::result::Result<(), String> {
        let options = self.options;
        let mut command = Command::new(&options.worker);
        command
            .args(arguments)
            .env("AOE_MAP_REQUEST", &options.request)
            .env("AOE_MAP_PACKAGE", &options.packages)
            .env("AOE_GEODATA_CACHE", &options.cache)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", options.worker.display()))?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return Err("worker streams are not piped".to_owned());
        };
        let streams = [spawn_tail(stdout, false), spawn_tail(stderr, true)];
        let status = child.wait().map_err(|error| error.to_string())?;
        let mut tail = VecDeque::new();
        for stream in streams {
            tail.extend(stream.join().unwrap_or_default());
        }
        if status.success() {
            return Ok(());
        }
        let skipped = tail.len().saturating_sub(FAILURE_TAIL_LINES);
        let shown: Vec<String> = tail.into_iter().skip(skipped).collect();
        Err(format!(
            "{} {} failed ({status}): {}",
            options.worker.display(),
            arguments.join(" "),
            shown.join(" | ")
        )
        .trim_end()
        .to_owned())
    }

    fn retrying(&self, step: &str) -> std::result::Result<(), String> {
        let attempts = self.options.attempts.max(1);
        let mut last = String::new();
        for attempt in 1..=attempts {
            match self.run(&[step]) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    eprintln!("game-map: {step} attempt {attempt}/{attempts} failed: {error}");
                    last = error;
                }
            }
            if attempt < attempts {
                thread::sleep(Duration::from_secs(self.options.retry_wait_seconds));
            }
        }
        Err(format!("{step} failed after {attempts} attempts: {last}"))
    }

    fn verify(&self, manifest: &Path) -> Result<()> {
        let options = Options {
            packages: manifest.to_path_buf(),
            ..self.options.clone()
        };
        Worker { options: &options }
            .run(&["map-verify"])
            .map_err(Into::into)
    }
}

/// Echo a child's stream as it arrives and keep its last lines for the failure message.
fn spawn_tail<R: std::io::Read + Send + 'static>(
    stream: R,
    to_stderr: bool,
) -> thread::JoinHandle<Vec<String>> {
    thread::spawn(move || {
        let mut tail = VecDeque::new();
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if to_stderr {
                eprintln!("{line}");
            } else {
                println!("{line}");
            }
            tail.push_back(line);
            if tail.len() > FAILURE_TAIL_LINES {
                tail.pop_front();
            }
        }
        tail.into()
    })
}
