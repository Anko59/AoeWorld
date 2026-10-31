//! A side's server: either one this run starts (own port, own log, stopped on
//! every return path) or an already running one named by URL. Also how a map
//! name becomes a stored package on each side.
use super::bookmarks::MapRequest;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const START_DEADLINE: Duration = Duration::from_secs(30);
const LOG_TAIL: usize = 4000;

/// A server this run owns. Dropping it kills it, so no return path leaves one.
pub(crate) struct Server {
    child: Child,
    pub(crate) url: String,
    log: PathBuf,
}

impl Server {
    /// `binary` serves `tree/web` (it serves `web/` relative to its working
    /// directory) with `packages` as its stored maps and `open` (a package's
    /// content hash) open from the start, on a free loopback port.
    ///
    /// The server only accepts an asset pack that is a child of the
    /// `local-assets/packs` of its working directory, so a tree without a
    /// `local-assets` (the exported base) gets a link to the checkout's.
    pub(crate) fn start(
        binary: &Path,
        tree: &Path,
        local_assets: &Path,
        pack: &str,
        packages: &Path,
        open: &str,
        log: &Path,
    ) -> Result<Self> {
        if !tree.join("local-assets").exists() {
            std::os::unix::fs::symlink(local_assets, tree.join("local-assets"))?;
        }
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        drop(listener);
        let child = Command::new(binary)
            .current_dir(tree)
            .env("AOE_BIND", address.to_string())
            .env("AOE_SCENARIO", "smoke")
            .envs(side_env(pack, &packages.canonicalize()?, open))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(log)?)
            .spawn()?;
        let mut server = Self {
            child,
            url: format!("http://{address}/"),
            log: log.to_path_buf(),
        };
        let deadline = Instant::now() + START_DEADLINE;
        while !healthy(address) {
            if let Some(status) = server.child.try_wait()? {
                return Err(
                    format!("the server exited early ({status}):\n{}", server.log_tail()).into(),
                );
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "the server at {address} was not healthy within {}s:\n{}",
                    START_DEADLINE.as_secs(),
                    server.log_tail()
                )
                .into());
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(server)
    }

    pub(crate) fn log_tail(&self) -> String {
        let text = fs::read_to_string(&self.log).unwrap_or_default();
        let start = text.len().saturating_sub(LOG_TAIL);
        let start = (start..text.len())
            .find(|i| text.is_char_boundary(*i))
            .unwrap_or(0);
        text[start..].to_owned()
    }

    /// Whether the server is still running (it should be, after the captures).
    pub(crate) fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SIGTERM first so it exits like a normal run, then make sure.
        if let Ok(pid) = i32::try_from(self.child.id()) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && matches!(self.child.try_wait(), Ok(None)) {
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn healthy(address: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    if stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut prefix = [0u8; 12];
    stream.read_exact(&mut prefix).is_ok() && &prefix == b"HTTP/1.1 200"
}

/// The environment a side's server runs with: its asset pack, its package
/// directory and the one package it opens at start (the capturing page's map).
pub(crate) fn side_env(pack: &str, packages: &Path, open: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "AOE_ASSET_PACK",
            Path::new("local-assets/packs")
                .join(pack)
                .display()
                .to_string(),
        ),
        ("AOE_MAP_PACKAGE_DIRECTORY", packages.display().to_string()),
        ("AOE_OPEN_MAP_PACKAGE", open.to_owned()),
        ("AOE_FOG_DISABLED", "1".to_owned()),
    ]
}

/// COMPARE_BEFORE / COMPARE_AFTER given as a URL: plain `http` on loopback
/// with an explicit port and nothing else, like SHOWCASE_APP_URL.
pub(crate) fn running_url(name: &str, value: &str) -> Result<String> {
    let url = url::Url::parse(value).map_err(|e| format!("{name}={value}: {e}"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    let plain = url.scheme() == "http"
        && url.port().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none();
    if !(loopback && plain) {
        return Err(format!(
            "{name}={value}: use `build`, or a running app as plain http://127.0.0.1:<port>/"
        )
        .into());
    }
    Ok(format!(
        "http://{}:{}/",
        url.host_str().unwrap_or_default(),
        url.port().unwrap_or(0)
    ))
}

/// A stored package's request has more fields (seed, compression, year…) than
/// the three that name a map; only those are read.
#[derive(Deserialize)]
struct StoredRequest {
    center_latitude_e7: i64,
    center_longitude_e7: i64,
    requested_side_meters: u64,
}

#[derive(Deserialize)]
struct Stored {
    request: StoredRequest,
}

/// A stored package that carries a map's request: its hash and file date.
struct Match {
    hash: String,
    request: String,
    modified: SystemTime,
}

/// The UTC calendar day of `time`, as `YYYY-MM-DD`.
pub(crate) fn utc_date(time: SystemTime) -> String {
    let days = i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86_400,
    )
    .unwrap_or_default();
    // Howard Hinnant's civil-from-days, for days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// For each map name, the content hash of the stored package in `directory`
/// whose request is the map's. Package files are `<64 hex>.json`. A map with
/// two such packages is refused: the run never picks one silently.
pub(crate) fn resolve_maps(
    directory: &Path,
    maps: &BTreeMap<String, MapRequest>,
    wanted: &[&str],
) -> Result<BTreeMap<String, String>> {
    let mut matches: BTreeMap<String, Vec<Match>> = BTreeMap::new();
    let entries = fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(hash) = name.strip_suffix(".json") else {
            continue;
        };
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        // A file this run cannot read as a package is the server's to judge
        // (stale local data fails its startup with the existing hint).
        let Ok(stored) = serde_json::from_slice::<Stored>(&fs::read(entry.path())?) else {
            continue;
        };
        for (map, request) in maps {
            let r = &stored.request;
            let same = request.center_latitude_e7 == r.center_latitude_e7
                && request.center_longitude_e7 == r.center_longitude_e7
                && request.requested_side_meters == r.requested_side_meters;
            if same && wanted.contains(&map.as_str()) {
                let modified = entry.metadata()?.modified()?;
                matches.entry(map.clone()).or_default().push(Match {
                    hash: hash.to_owned(),
                    request: format!(
                        "{}/{} e7, {} m",
                        r.center_latitude_e7, r.center_longitude_e7, r.requested_side_meters
                    ),
                    modified,
                });
            }
        }
    }
    let mut found = BTreeMap::new();
    for map in wanted {
        let Some(list) = matches.remove(*map) else {
            return Err(format!(
                "no stored package for map `{map}` in {}: copy a current-format package there (or point COMPARE_BEFORE_PACKAGES / COMPARE_AFTER_PACKAGES at one); `make map-generate` makes them",
                directory.display()
            )
            .into());
        };
        if let [only] = list.as_slice() {
            found.insert((*map).to_owned(), only.hash.clone());
            continue;
        }
        let mut lines: Vec<String> = list
            .iter()
            .map(|m| {
                format!(
                    "  {} request {} modified {}",
                    m.hash,
                    m.request,
                    utc_date(m.modified)
                )
            })
            .collect();
        lines.sort();
        return Err(format!(
            "several stored packages for map `{map}` in {}:\n{}\nkeep one there, or point COMPARE_BEFORE_PACKAGES / COMPARE_AFTER_PACKAGES at a directory holding one",
            directory.display(),
            lines.join("\n")
        )
        .into());
    }
    Ok(found)
}
