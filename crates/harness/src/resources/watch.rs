//! Temporary cold-build safety watchdog, independent of Docker polling.
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 2 {
        return Err("watch requires DIRECTORY LABEL".into());
    }
    let dir = PathBuf::from(&args[0]);
    let label = &args[1];
    fs::create_dir_all(&dir)?;
    let mut free = Command::new("free")
        .args(["--kibi", "--seconds", "1"])
        .stdout(File::create(dir.join("free-1hz.txt"))?)
        .spawn()?;
    let mut events = Command::new("docker")
        .args([
            "events",
            "--filter",
            &format!("label=aoe.noqueue.stress={label}"),
            "--format",
            "{{.TimeNano}} {{.Action}} {{.Actor.ID}}",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    let reader = BufReader::new(events.stdout.take().ok_or("missing Docker event stream")?);
    let active = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let ids = active.clone();
    let mut event_file = File::create(dir.join("container-events.txt"))?;
    thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            let _ = writeln!(event_file, "{line}");
            let _ = event_file.flush();
            let words: Vec<_> = line.split_whitespace().collect();
            if words.len() != 3 {
                continue;
            }
            let mut set = ids.lock().unwrap_or_else(|error| error.into_inner());
            match words[1] {
                "start" => {
                    set.insert(words[2].to_owned());
                }
                "die" => {
                    set.remove(words[2]);
                }
                _ => {}
            }
        }
    });
    let mut samples = File::create(dir.join("watchdog-100ms.tsv"))?;
    writeln!(
        samples,
        "seconds\tavailable_kib\tswap_used_kib\tactive_own_containers"
    )?;
    let start = Instant::now();
    let mut stopped = false;
    fs::write(dir.join("watchdog-ready"), "ready\n")?;
    while !dir.join("finished").exists() {
        let text = fs::read_to_string("/proc/meminfo")?;
        let value = |key: &str| -> Result<u64, Box<dyn std::error::Error>> {
            Ok(text
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .ok_or("missing memory metric")?
                .split_whitespace()
                .next()
                .ok_or("empty memory metric")?
                .parse()?)
        };
        let available = value("MemAvailable:")?;
        let swap = value("SwapTotal:")? - value("SwapFree:")?;
        let own: Vec<String> = active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned()
            .collect();
        writeln!(
            samples,
            "{:.3}\t{available}\t{swap}\t{}",
            start.elapsed().as_secs_f64(),
            own.len()
        )?;
        samples.flush()?;
        if available < 1_572_864 && !stopped {
            stopped = true;
            validate_ids(&own)?;
            fs::write(
                dir.join("safety-stop.txt"),
                format!(
                    "MemAvailable={available} KiB; swap_used={swap} KiB; elapsed={:.3}s; own_ids={own:?}\n",
                    start.elapsed().as_secs_f64()
                ),
            )?;
            if !own.is_empty() {
                let out = Command::new("docker").arg("kill").args(&own).output()?;
                fs::write(dir.join("safety-kill.stdout"), out.stdout)?;
                fs::write(dir.join("safety-kill.stderr"), out.stderr)?;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = free.kill();
    let _ = free.wait();
    let _ = events.kill();
    let _ = events.wait();
    Ok(())
}

fn validate_ids(ids: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if ids.len() > 4
        || ids
            .iter()
            .any(|id| id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("refusing to kill anything outside the four owned containers".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/watch.rs"]
mod tests;
