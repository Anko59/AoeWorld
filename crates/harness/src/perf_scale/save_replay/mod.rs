//! Repeat a fixed save workload with hardware instructions and thread CPU time.
mod counts;
mod orders;
mod progress;
use super::stats::Series;
use aoe_server::{GameplayService, queue_json, replay::Recording, scheduler_json};
use clap::Args;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

#[derive(Args, Debug)]
pub struct Options {
    #[arg(long)]
    save: PathBuf,
    #[arg(long)]
    package_directory: PathBuf,
    #[arg(long)]
    recording: PathBuf,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=10))]
    runs: u32,
    #[arg(long, default_value = ".cache/save-replay/report.json")]
    out: PathBuf,
}
fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}
fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}
fn once(options: &Options, recording: &Recording) -> Result<Value, String> {
    let (service, restored) =
        GameplayService::from_saved_replay(&options.save, &options.package_directory)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let initial = service.inspect_scale_world(|w| w.canonical_hash()).await;
        let mut progress = progress::Progress::default();
        service.inspect_scale_world(|w| progress.sample(w)).await;
        let mut divergence = (initial != recording.initial_hash).then(|| recording.ticks[0].tick);
        let mut total = Vec::<u64>::new();
        let mut cpu = Series::default();
        let mut phases_cpu = BTreeMap::<&str, Series>::new();
        let mut phases = BTreeMap::<&str, Vec<u64>>::new();
        let mut hashes = Vec::new();
        let mut raw = Vec::new();
        let mut commands = 0;
        let mut navigation_calls = BTreeMap::<String, u64>::new();
        let mut kinds = BTreeMap::<String, (u64, u64)>::new();
        let mut refused = Vec::new();
        let mut census = [0u64; 8];
        let mut timing_commands = BTreeMap::<String, (u64, u64)>::new();
        for tick in &recording.ticks {
            commands += tick.commands.len();
            let hash = service.replay_step(tick, &mut |timing| {
                total.push(timing.instructions);
                timing_commands = timing.commands.clone();
                refused.extend(timing.refused.iter().cloned());
                cpu.push_ms(timing.cpu_ns.into());
                for (name, nanos) in &timing.phases_cpu_ns { phases_cpu.entry(name).or_default().push_ms((*nanos).into()); }
                for (name, count) in &timing.phases_instructions { phases.entry(name).or_default().push(*count); }
                for (path, calls) in &timing.navigation_calls { *navigation_calls.entry(path.clone()).or_default() += *calls; }
                raw.push(json!({"tick": tick.tick, "instructions": timing.instructions, "phases_instructions": timing.phases_instructions, "cpu_ns": timing.cpu_ns, "phases_cpu_ns": timing.phases_cpu_ns, "navigation_calls": timing.navigation_calls}));
            }).await.map_err(|e| e.to_string())?;
            for (name, (count, instructions)) in &timing_commands {
                let row = kinds.entry(name.clone()).or_default();
                row.0 += count;
                row.1 += instructions;
            }
            timing_commands.clear();
            let movers = service.inspect_scale_world(|w| { progress.sample(w); w.order_mover_census() }).await;
            for (total, now) in census.iter_mut().zip(movers) { *total += now; }
            if hash != tick.hash && divergence.is_none() { divergence = Some(tick.tick + 1); }
            hashes.push(json!({"tick": tick.tick + 1, "hash": hex(&hash)}));
        }
        // Include zeros for phases absent from individual ticks in percentile summaries.
        for series in phases.values_mut() { series.resize(recording.ticks.len(), 0); }
        let phase_summary: BTreeMap<_, _> = phases.iter().map(|(name, series)| (*name, counts::summary(series))).collect();
        let phases_cpu_summary: BTreeMap<_, _> = phases_cpu.iter().map(|(name, series)| (*name, series.summary())).collect();
        let attribution = service.inspect_scale_world(|w| orders::report(w, &kinds, &census, recording.ticks.len())).await;
        // Route-scheduler state at the end of the window, read off-tick like
        // the mover census: the same counters the live probe reports, on a
        // window that repeats. No human seat sends orders in this recording.
        let routes = service
            .inspect_scale_world(|w| {
                let mut routes = scheduler_json(&w.route_scheduler_stats(), w.tick().0);
                routes["queue"] = queue_json(&w.route_queue_breakdown(), &[]);
                routes
            })
            .await;
        Ok(json!({ "order_attribution": attribution, "refused_commands": refused, "initial_hash": hex(&initial), "total_cpu_ms_per_tick": cpu.summary(), "phases_cpu_ms": phases_cpu_summary, "total_instructions_per_tick": counts::summary(&total), "phases_instructions": phase_summary,
            "first_divergence_tick": divergence, "final_hash": hashes.last().ok_or("empty replay")?["hash"],
            "movement_progress": progress.summary(recording.ticks.last().ok_or("empty replay")?.tick + 1),
            "navigation_calls": navigation_calls, "routes": routes,
            "hashes": hashes, "raw": raw, "commands": commands, "restore": format!("{restored:?}") }))
    })
}
fn spread(runs: &[Value], series: &str, mean: &str) -> Result<f64, String> {
    let means: Vec<f64> = runs
        .iter()
        .map(|r| r[series][mean].as_f64().ok_or("missing replay mean"))
        .collect::<Result<_, _>>()?;
    let low = means.iter().copied().fold(f64::INFINITY, f64::min);
    let high = means.iter().copied().fold(0.0, f64::max);
    Ok((high - low) / (means.iter().sum::<f64>() / means.len() as f64) * 100.0)
}
pub fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let recording = Recording::read(&options.recording).map_err(|e| e.to_string())?;
    // One worker executes both the tick and all Rayon proposals on its own thread.
    // The user-only per-thread PMU counter includes all simulation instruction work.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build()?;
    let mut runs = Vec::new();
    for index in 0..options.runs {
        let report = pool.install(|| once(&options, &recording))?;
        println!(
            "save-replay run {}: {} instructions/tick; {} CPU ms/tick; divergence {}; final {}",
            index + 1,
            report["total_instructions_per_tick"]["mean"],
            report["total_cpu_ms_per_tick"]["mean_ms"],
            report["first_divergence_tick"],
            report["final_hash"]
        );
        runs.push(report);
    }
    let spread = spread(&runs, "total_instructions_per_tick", "mean")?;
    let cpu_spread = self::spread(&runs, "total_cpu_ms_per_tick", "mean_ms")?;
    let repeat_hashes_agree = runs.iter().all(|run| run["hashes"] == runs[0]["hashes"]);
    let report = json!({"revision": git(&["rev-parse", "HEAD"]), "dirty": !git(&["status", "--porcelain"]).is_empty(),
        "cpu_clock": "CLOCK_THREAD_CPUTIME_ID", "counter": "PERF_COUNT_HW_INSTRUCTIONS", "exclude_kernel": true, "exclude_hv": true,
        "inherit": false, "pinned_counter": true, "multiplexed": false, "rayon_workers": 1, "ticks": recording.ticks.len(),
        "source_diff_blake3": blake3::hash(git(&["diff", "HEAD", "--binary"]).as_bytes()).to_hex().to_string(),
        "save_blake3": blake3::hash(&fs::read(&options.save)?).to_hex().to_string(),
        "map_hash": recording.map_hash.map(|h| hex(&h)),
        "repeat_hashes_agree": repeat_hashes_agree,
        "recording_blake3": blake3::hash(&fs::read(&options.recording)?).to_hex().to_string(),
        "spread_percent": spread, "cpu_spread_percent": cpu_spread, "runs": runs,
        "limits": "User-only retired instructions alongside thread CPU time; single worker, inclusive spans overlap. Excludes load, hashing, fanout, AI, save/recording I/O and transport; lazy terrain work remains measured; not live-match or dedicated-hardware qualification."});
    fs::create_dir_all(options.out.parent().ok_or("output needs parent")?)?;
    fs::write(&options.out, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "spread {spread:.3}% (max-min)/mean; report {}",
        options.out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests;
