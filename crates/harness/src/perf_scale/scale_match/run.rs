//! The orchestration of one `scale-match` run.
use super::{
    BoxError, MapSource, Options, browser,
    hosts::{start_hosts, wait_connected},
    http, procs,
    report::{self, Sample, TickRow},
};
use crate::process;
use aoe_map::{MapRequest, Ratio};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const WORK: &str = "target/scale-match";
/// A local development secret: the server listens on loopback only.
const ADMIN_TOKEN: &str = "scale-match-local-0123456789";
const SAMPLE_EVERY: Duration = Duration::from_secs(5);
const BUILD_DEADLINE: Duration = Duration::from_secs(60 * 60);
const MATCH_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

struct Server {
    child: Child,
    address: String,
}

impl Server {
    fn get(&self, path: &str) -> Result<Value, BoxError> {
        let response = http::request(
            &self.address,
            "GET",
            path,
            &[("X-AoeWorld-Admin-Token", ADMIN_TOKEN)],
            None,
            REQUEST_TIMEOUT,
        )?;
        if response.status != 200 {
            return Err(format!("GET {path}: status {}", response.status).into());
        }
        Ok(serde_json::from_slice(&response.body)?)
    }

    fn exited(&mut self) -> Option<String> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| format!("the server exited: {status}"))
    }
}

/// Stops the bot-host and browser containers on every exit path, including an
/// error return after they started. Killing a container that already exited
/// is harmless.
struct Containers {
    bots_prefix: String,
    browser: String,
}

impl Drop for Containers {
    fn drop(&mut self) {
        let listed = Command::new("docker")
            .args(["ps", "-q", "--filter"])
            .arg(format!("name={}", self.bots_prefix))
            .output();
        let mut command = Command::new("docker");
        command.args(["kill", &self.browser]);
        if let Ok(output) = listed {
            command.args(String::from_utf8_lossy(&output.stdout).split_whitespace());
        }
        let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn france_request(tiles: u32, seed: u64) -> Result<Value, BoxError> {
    let request = MapRequest {
        center_latitude_e7: 466_000_000,
        center_longitude_e7: 25_000_000,
        requested_side_meters: u64::from(tiles) * 2 * 30,
        compression: Ratio::new(30, 1)?,
        seed,
        ..MapRequest::default()
    };
    Ok(serde_json::to_value(request)?)
}

/// The `POST /match` body for these options.
pub(super) fn match_body(options: &Options) -> Result<Value, BoxError> {
    let mut slots = vec!["human"; options.humans];
    slots.resize(options.players, "ai");
    let mut body = json!({
        "slots": slots,
        "population_cap": options.population_cap,
    });
    if options.start_villagers + options.start_militia > 0 {
        body["start_units"] =
            json!({ "villagers": options.start_villagers, "militia": options.start_militia });
    }
    match options.map {
        MapSource::Procedural => body["map"] = france_request(options.tiles, options.seed)?,
        MapSource::Package => {
            body["map_hash"] = json!(
                options
                    .map_hash
                    .as_deref()
                    .ok_or("--map package needs --map-hash")?
            );
        }
        MapSource::Flat => {
            body["flat"] = json!({ "tiles_per_side": options.tiles, "seed": options.seed });
        }
    }
    Ok(body)
}

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn start_server(options: &Options, root: &Path, work: &Path) -> Result<Server, BoxError> {
    fs::create_dir_all(&options.package_directory)?;
    let log = fs::File::create(work.join("server.log"))?;
    let address = format!("127.0.0.1:{}", options.port);
    let mut command = Command::new(root.join("target/release/aoe-server"));
    command
        .current_dir(root)
        .env("AOE_BIND", &address)
        .env("AOE_MATCH_TOKEN", ADMIN_TOKEN)
        .env(
            "AOE_MAP_PACKAGE_DIRECTORY",
            fs::canonicalize(&options.package_directory)?,
        )
        .env("AOE_BUILD_SHA", git(&["rev-parse", "HEAD"]))
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    if let Some(pack) = options
        .asset_pack
        .clone()
        .or_else(|| std::env::var_os("AOE_ASSET_PACK").map(PathBuf::from))
    {
        command.env("AOE_ASSET_PACK", pack);
    }
    let mut server = Server {
        child: command.spawn()?,
        address,
    };
    let started = Instant::now();
    while std::net::TcpStream::connect(&server.address).is_err() {
        if let Some(reason) = server.exited() {
            return Err(reason.into());
        }
        if started.elapsed() > Duration::from_secs(120) {
            return Err("the server did not listen within 120 s".into());
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(server)
}

fn pull_ticks(server: &Server, cursor: &mut u64, rows: &mut Vec<TickRow>) -> Result<(), BoxError> {
    let page = server.get(&format!("/match/stats?since={cursor}"))?;
    rows.extend(
        page["ticks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(TickRow::from_json),
    );
    *cursor = page["next"].as_u64().unwrap_or(*cursor);
    Ok(())
}

fn mean_max(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let all: Vec<f64> = values.collect();
    let mean = all.iter().sum::<f64>() / all.len().max(1) as f64;
    (
        crate::perf_scale::stats::round(mean),
        crate::perf_scale::stats::round(all.iter().copied().fold(0.0, f64::max)),
    )
}

pub(super) fn run(options: &Options) -> Result<(), BoxError> {
    let root = std::env::current_dir()?;
    let work = root.join(WORK);
    fs::create_dir_all(&work)?;
    let revision = git(&["rev-parse", "HEAD"]);
    let dirty = !git(&["status", "--porcelain"]).is_empty();
    println!("scale-match: building the release server");
    process::run(
        "cargo",
        &["build", "--release", "--locked", "-p", "aoe-server"],
        BUILD_DEADLINE,
    )?;
    let mut server = start_server(options, &root, &work)?;

    println!(
        "scale-match: creating the match ({} slots, {} tiles)",
        options.players, options.tiles
    );
    let created = Instant::now();
    let response = http::request(
        &server.address,
        "POST",
        "/match",
        &[("X-AoeWorld-Admin-Token", ADMIN_TOKEN)],
        Some(&serde_json::to_vec(&match_body(options)?)?),
        MATCH_TIMEOUT,
    )?;
    let create_seconds = created.elapsed().as_secs_f64();
    if response.status != 200 {
        return Err(format!(
            "POST /match: status {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        )
        .into());
    }
    let info: Value = serde_json::from_slice(&response.body)?;
    let tokens: Vec<String> = info["slots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|slot| slot["kind"] == "ai")
        .filter_map(|slot| slot["token"].as_str().map(str::to_owned))
        .collect();
    println!(
        "scale-match: match created in {create_seconds:.0} s, {} AI slots",
        tokens.len()
    );

    let (mut cursor, mut rows) = (0u64, Vec::new());
    // The first record of the new world is the start of the run.
    let _ = pull_ticks(&server, &mut cursor, &mut rows);
    rows.clear();
    let _containers = Containers {
        bots_prefix: format!("aoe-scale-bots-{}-", options.port),
        browser: browser::container_name(options),
    };
    let mut hosts = start_hosts(options, &root, &work, &tokens)?;
    let host_names: Vec<String> = hosts.iter().map(|host| host.name.clone()).collect();
    // Every host reports once all its bots hold a session: that is the connect
    // time, and the measured window starts after it, without earlier ticks.
    let connect_seconds = wait_connected(&mut hosts)?;
    println!("scale-match: all bots connected {connect_seconds:.1} s after the hosts started");
    let _ = pull_ticks(&server, &mut cursor, &mut rows);
    rows.clear();

    let wall = Duration::from_secs_f64(options.minutes * 60.0);
    let run_started = Instant::now();
    let mut samples = Vec::<Sample>::new();
    let mut browser_child: Option<Child> = None;
    let browser_out = work.join("browser.json");
    let _ = fs::remove_file(&browser_out);
    let mut last_cpu = procs::process(server.child.id()).map_or(0.0, |(seconds, _)| seconds);
    let mut last_sample = Instant::now();
    let mut failure = None;
    while run_started.elapsed() < wall {
        if let Some(reason) = server.exited() {
            failure = Some(reason);
            break;
        }
        if let Err(error) = pull_ticks(&server, &mut cursor, &mut rows) {
            failure = Some(error.to_string());
            break;
        }
        if options.browser
            && browser_child.is_none()
            && run_started.elapsed() >= Duration::from_secs(options.browser_delay)
        {
            println!("scale-match: starting the browser measurement");
            browser_child = Some(browser::spawn(options, &root, &browser_out)?);
        }
        if last_sample.elapsed() >= SAMPLE_EVERY {
            // The CPU reading and its time are taken together; the slow docker
            // call below must not stretch the next interval.
            let now = Instant::now();
            let cpu_total = procs::process(server.child.id());
            let (cpu_cores, rss) = match cpu_total {
                Some((seconds, rss)) => {
                    let cores = ((seconds - last_cpu) / (now - last_sample).as_secs_f64()).max(0.0);
                    last_cpu = seconds;
                    (cores, rss)
                }
                None => (0.0, 0),
            };
            let bots = procs::containers(&host_names).unwrap_or_default();
            last_sample = now;
            samples.push(Sample {
                seconds: run_started.elapsed().as_secs_f64(),
                tick: rows.last().map_or(0, |row| row.tick),
                units: rows.last().map_or(0, |row| row.units),
                server_cpu_cores: cpu_cores,
                server_rss_mib: rss,
                bots_cpu_cores: bots.iter().map(|c| c.cores).sum(),
                bots_rss_mib: bots.iter().map(|c| c.rss_mib).sum(),
            });
            if let Some(sample) = samples.last() {
                println!(
                    "  t={:4.0}s tick {} units {} server {:.2} cores {} MiB bots {:.2} cores",
                    sample.seconds,
                    sample.tick,
                    sample.units,
                    sample.server_cpu_cores,
                    sample.server_rss_mib,
                    sample.bots_cpu_cores
                );
            }
        }
        thread::sleep(Duration::from_millis(900));
    }
    let wall_seconds = run_started.elapsed().as_secs_f64();
    let _ = pull_ticks(&server, &mut cursor, &mut rows);

    // The bots stop by themselves at their own time limit; give them a moment.
    let grace = Instant::now();
    for host in &mut hosts {
        while host.child.try_wait()?.is_none() && grace.elapsed() < Duration::from_secs(90) {
            thread::sleep(Duration::from_millis(500));
        }
        if host.child.try_wait()?.is_none() {
            let _ = Command::new("docker").args(["kill", &host.name]).output();
            let _ = host.child.wait();
        }
    }
    let browser = match browser_child {
        Some(mut child) => {
            let waited = Instant::now();
            while child.try_wait()?.is_none() && waited.elapsed() < Duration::from_secs(300) {
                thread::sleep(Duration::from_millis(500));
            }
            if child.try_wait()?.is_none() {
                let _ = Command::new("docker")
                    .args(["kill", &browser::container_name(options)])
                    .output();
            }
            fs::read(&browser_out)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_else(|| json!({ "error": "the browser script wrote no result" }))
        }
        None if options.browser => json!({ "error": "the run ended before the browser started" }),
        None => Value::Null,
    };
    let bot_reports: Vec<Value> = hosts
        .iter()
        .filter_map(|host| fs::read(&host.report).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|value| value["bots"].clone())
        .collect();
    let (server_mean, server_max) = mean_max(samples.iter().map(|s| s.server_cpu_cores));
    let (bots_mean, bots_max) = mean_max(samples.iter().map(|s| s.bots_cpu_cores));
    let first_units = rows.first().map_or(0, |row| row.units);
    let out = json!({
        "schema": 1,
        "label": options.label,
        "revision": revision,
        "dirty": dirty,
        "failure": failure,
        "config": {
            "players": options.players, "humans": options.humans, "tiles": options.tiles,
            "population_cap": options.population_cap, "minutes": options.minutes,
            "hosts": options.hosts, "start_villagers": options.start_villagers,
            "start_militia": options.start_militia, "seed": options.seed,
        },
        "setup": {
            "map_kind": format!("{:?}", options.map).to_lowercase(),
            "content_hash": info["content_hash"],
            "match_create_seconds": crate::perf_scale::stats::round(create_seconds),
            "bots_connect_seconds": crate::perf_scale::stats::round(connect_seconds),
            "seeded_units": info["seeded_units"],
            "partial_starting_kits": info["partial_starting_kits"],
            "units_at_first_tick": first_units,
            "start_tiles": info["slots"].as_array().map(|slots| slots.iter().map(|s| s["start_tile"].clone()).collect::<Vec<_>>()),
        }, "run": {
            "wall_seconds": crate::perf_scale::stats::round(wall_seconds),
            "units_gained_by_play": rows.iter().map(|r| r.units).max().unwrap_or(0).saturating_sub(first_units),
            "window_note": "tick_ms is the whole tick (orders, advance, per-session views); late_start counts a tick that began over 50 ms after its schedule",
            "ticks_total": rows.len(),
            "missed_total": rows.iter().filter(|r| r.missed).count(),
            "window": report::window(&rows, wall_seconds),
        },
        "machine": {
            "server_cpu_mean_cores": server_mean, "server_cpu_max_cores": server_max,
            "server_rss_max_mib": samples.iter().map(|s| s.server_rss_mib).max().unwrap_or(0),
            "bots_cpu_mean_cores": bots_mean, "bots_cpu_max_cores": bots_max,
            "bots_rss_max_mib": crate::perf_scale::stats::round(samples.iter().map(|s| s.bots_rss_mib).fold(0.0, f64::max)),
            "cores": std::thread::available_parallelism().map_or(0, usize::from),
        },
        "bots": report::bot_totals(&bot_reports),
        "browser": browser,
        "per_minute": report::per_minute(&rows, wall_seconds),
        "timeline": report::timeline(&samples),
    });
    finish(options, &root, out)
}

/// Flattens `run.window` into `run` for the summary, writes both files.
fn finish(options: &Options, root: &Path, mut out: Value) -> Result<(), BoxError> {
    let window = out["run"]["window"].clone();
    if let (Some(run), Some(window)) = (out["run"].as_object_mut(), window.as_object()) {
        for (key, value) in window {
            run.insert(key.clone(), value.clone());
        }
        run.remove("window");
    }
    let path = options.out.clone().unwrap_or_else(|| {
        let stamp = out["revision"]
            .as_str()
            .unwrap_or("rev")
            .chars()
            .take(8)
            .collect::<String>();
        root.join(format!(
            "reports/scale/match/match-{}-{stamp}.json",
            options.label
        ))
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_vec(&out)?)?;
    println!("scale-match: wrote {}", path.display());
    if !options.no_summary {
        write_summary(&options.summary, &out)?;
        println!("scale-match: updated {}", options.summary.display());
    }
    if let Some(failure) = out["failure"].as_str() {
        return Err(format!("the run was cut short: {failure}").into());
    }
    Ok(())
}

/// Rewrites everything after the marker line; text before it is kept.
pub(super) fn write_summary(path: &Path, report: &Value) -> Result<(), BoxError> {
    const MARKER: &str = "<!-- scale-match:latest -->";
    let existing = fs::read_to_string(path).unwrap_or_default();
    let head = existing
        .find(MARKER)
        .map_or(existing.as_str(), |at| &existing[..at]);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{head}{MARKER}\n\n{}", report::markdown(report)),
    )?;
    Ok(())
}
