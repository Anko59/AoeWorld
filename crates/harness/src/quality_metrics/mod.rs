//! Deterministic quality measurements: integer simulation time, no acceptance hash.
mod combat;
mod compile_load;
mod march;
mod report;
mod setup;
mod source;
mod stats;
mod video;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
use clap::Args;
use march::OrderPath;
use serde_json::json;
use std::{path::PathBuf, process::Command};

#[derive(Args, Debug)]
pub(crate) struct Options {
    #[arg(long, default_value = ".cache/dev-replay/packages")]
    package_directory: PathBuf,
    #[arg(
        long,
        default_value = "fdc59853e2404cb5c7fb245f21768970aeb01febe20fd4f15dc96ff522fc9a49"
    )]
    source_hash: String,
    #[arg(long, default_value = "run")]
    label: String,
    #[arg(long, default_value_t=1, value_parser=clap::value_parser!(u32).range(1..=64))]
    workers: u32,
    #[arg(long)]
    compare: Option<PathBuf>,
    #[arg(long)]
    frames: Option<PathBuf>,
    #[arg(long, default_value = "reports/quality")]
    out_directory: PathBuf,
    /// A focused diagnostic selection; the default is the complete matrix.
    #[arg(long)]
    scenario: Option<String>,
    /// Keep a real Cargo build active in this same resource-capped container.
    #[arg(long)]
    compile_load: bool,
}
fn git(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err("git metadata command failed".into());
    }
    Ok(output.stdout)
}
pub(crate) fn run(options: Options) -> Result<()> {
    let revision = String::from_utf8(git(&["rev-parse", "HEAD"])?)?
        .trim()
        .to_owned();
    let source_digest = report::source_digest(std::path::Path::new("."))?;
    let dirty = !git(&["status", "--porcelain"])?.is_empty();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.workers as usize)
        .build()?;
    let host_load_start = host_load();
    let mut compiling = options
        .compile_load
        .then(compile_load::CompileLoad::start)
        .transpose()?;
    if let Some(load) = &mut compiling {
        load.live()?;
    }
    let mut metrics = Vec::new();
    for scenario in [
        "long",
        "quality-march",
        "source-river",
        "attack-move-defended",
        "melee",
        "ranged",
    ] {
        if options.scenario.as_ref().is_some_and(|v| v != scenario) {
            continue;
        }
        for path in OrderPath::ALL {
            if path == OrderPath::Steward
                && matches!(scenario, "attack-move-defended" | "melee" | "ranged")
            {
                continue;
            }
            println!("quality: {scenario} / {}", path.name());
            // Boxed errors stay on this thread; only Send values cross the pool boundary.
            let value = pool.install(|| -> std::result::Result<serde_json::Value, String> {
                let run = || -> Result<serde_json::Value> {
                    if matches!(scenario, "attack-move-defended" | "melee" | "ranged") {
                        return combat::measure(scenario, path, options.frames.as_deref());
                    }
                    let scene = if scenario == "source-river" {
                        let (package, provider) =
                            source::load(&options.package_directory, &options.source_hash)?;
                        let mut world =
                            aoe_simulation::GameWorld::from_page_provider(package, provider)?;
                        let (start, end) = source::crossing(&mut world)?;
                        aoe_simulation::terrain_fixtures::reset_measurement_actors(&mut world);
                        march::populate(
                            world,
                            start,
                            end,
                            path,
                            aoe_simulation::UnitKind::Militia,
                            8000,
                        )?
                    } else {
                        march::prepare(scenario, path)?
                    };
                    scene.measure(scenario, options.frames.as_deref())
                };
                run().map_err(|e| format!("{scenario}/{}: {e}", path.name()))
            })?;
            println!("{}", serde_json::to_string(&value)?);
            metrics.push(value);
        }
    }
    if metrics.is_empty() {
        return Err("scenario selection matched no quality cases".into());
    }
    let metrics = serde_json::to_value(metrics)?;
    let comparison = options
        .compare
        .as_ref()
        .map(|p| report::compare(p, &metrics));
    if let Some(load) = &mut compiling {
        load.live()?;
    }
    let compilation = compiling.as_ref().map(|load| json!({"pid": load.pid(), "log": load.log,
        "active_at_start": true, "active_at_end": true,
        "workload": "fresh target, unused unique cfg, cargo build --locked --release -p aoe-harness; same container"}));
    let report = json!({"schema":1,"revision":revision,"dirty":dirty,"measurement_source_blake3":source_digest,
        "compilation": compilation, "host_load_start": host_load_start, "host_load_end": host_load(),
        "source_package_hash":options.source_hash,"workers":options.workers,"label":options.label,
        "repeatability_reference":options.compare,"repeatability_equal":comparison.as_ref().map(|r|r.is_ok()),
        "units":"integer ticks; 1024 subunits/tile; HP quarters; ppm ratios; milli rates",
        "limits":"Source river uses locked France pages; other scenarios are synthetic prepared terrain. Controlled cohorts, no live match or hardware qualification. Steward transfers villagers, General leads military squads. Combat has no Steward attack path.",
        "metrics":metrics});
    let path = report::write_history(&options.out_directory, &revision, &options.label, &report)?;
    println!("quality report: {}", path.display());
    if let Some(result) = comparison {
        result?;
    }
    Ok(())
}
fn host_load() -> Option<Vec<String>> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .map(|text| text.split_whitespace().take(3).map(str::to_owned).collect())
}
#[cfg(test)]
mod tests;
