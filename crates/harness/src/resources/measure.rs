//! Host-side observations for local resource qualification.
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Read the kernel's final peak before Docker removes the container's cgroup.
pub(super) fn container(args: &[String]) -> Result<i32> {
    let (program, args) = args.split_first().ok_or("missing container command")?;
    let status = Command::new(program).args(args).status()?;
    let peak =
        fs::read_to_string("/sys/fs/cgroup/memory.peak").unwrap_or_else(|_| "unavailable".into());
    let events = fs::read_to_string("/sys/fs/cgroup/memory.events").unwrap_or_default();
    eprintln!("resource: container_peak_bytes={}", peak.trim());
    if events
        .lines()
        .any(|line| line.strip_prefix("oom_kill ").is_some_and(|n| n != "0"))
    {
        eprintln!("resource: container exceeded its memory cap (kernel OOM kill)");
    }
    Ok(status.code().unwrap_or(137))
}

pub(super) fn available_kib(text: &str) -> Result<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))
        .and_then(|value| value.split_whitespace().next())
        .ok_or("MemAvailable missing")?
        .parse()
        .map_err(Into::into)
}

pub(super) fn cgroup_directory(text: &str) -> Result<PathBuf> {
    let relative = text
        .lines()
        .find_map(|line| line.strip_prefix("0::/"))
        .ok_or("unified cgroup missing")?;
    if Path::new(relative)
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("invalid cgroup path".into());
    }
    Ok(Path::new("/sys/fs/cgroup").join(relative))
}

pub(super) fn parse_job(text: &str) -> Result<(PathBuf, String)> {
    let (root, target) = text.split_once('=').ok_or("job must be WORKTREE=TARGET")?;
    if root.is_empty()
        || target.is_empty()
        || target.starts_with('-')
        || !target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("job needs an explicit worktree and one make target".into());
    }
    Ok((root.into(), target.into()))
}

struct Job {
    child: Child,
    started: Instant,
    root: PathBuf,
    target: String,
}

fn context(output: &mut File) -> Result<()> {
    for command in [
        vec!["uptime"],
        vec!["free", "-g"],
        vec!["docker", "stats", "--no-stream"],
    ] {
        let result = Command::new(command[0]).args(&command[1..]).output()?;
        output.write_all(&result.stdout)?;
        output.write_all(&result.stderr)?;
    }
    Ok(())
}

/// Docker's cgroup peak includes anonymous memory and page cache. Retain the maximum
/// observed peak per container; short containers may disappear between samples.
fn sample_containers(peaks: &mut BTreeMap<String, (String, u64)>) -> Result<()> {
    let ids = Command::new("docker")
        .args(["ps", "-q", "--no-trunc"])
        .output()?;
    if !ids.status.success() {
        return Err("docker ps failed".into());
    }
    let ids = String::from_utf8(ids.stdout)?;
    let ids: Vec<_> = ids.split_whitespace().collect();
    if ids.is_empty() {
        return Ok(());
    }
    let inspect = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{.Id}} {{.State.Pid}} {{.Config.WorkingDir}}",
        ])
        .args(ids)
        .output()?;
    for line in String::from_utf8(inspect.stdout)?.lines() {
        let mut fields = line.splitn(3, ' ');
        let (Some(id), Some(pid), Some(root)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if pid == "0" {
            continue;
        }
        let cgroup = fs::read_to_string(format!("/proc/{pid}/cgroup"));
        let Ok(text) = cgroup else { continue };
        let Ok(directory) = cgroup_directory(&text) else {
            continue;
        };
        if let Ok(text) = fs::read_to_string(directory.join("memory.peak"))
            && let Ok(bytes) = text.trim().parse::<u64>()
        {
            let peak = peaks.entry(id.into()).or_insert((root.into(), 0));
            peak.1 = peak.1.max(bytes);
        }
    }
    Ok(())
}

/// Run one or more actual Make jobs concurrently on the host. This observes resource
/// use; it does not turn synthetic jobs into performance qualification.
pub(super) fn run(args: Vec<String>) -> Result<()> {
    let mut output = None;
    let mut requested = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().map(PathBuf::from),
            "--job" => requested.push(parse_job(args.next().ok_or("missing --job value")?)?),
            _ => return Err(format!("unknown measurement argument: {arg}").into()),
        }
    }
    if requested.is_empty() {
        return Err("provide --job WORKTREE=TARGET".into());
    }
    let directory = output.ok_or("provide --output DIRECTORY")?;
    fs::create_dir_all(&directory)?;
    let mut context_file = File::create(directory.join("context.txt"))?;
    context(&mut context_file)?;
    let mut observations = File::create(directory.join("memory.tsv"))?;
    writeln!(observations, "seconds\tMemAvailable_kib")?;
    let mut results = File::create(directory.join("jobs.tsv"))?;
    writeln!(results, "worktree\ttarget\tseconds\texit")?;
    let mut jobs = Vec::new();
    for (index, (root, target)) in requested.into_iter().enumerate() {
        let log = File::create(directory.join(format!("job-{index}-{target}.log")))?;
        let started = Instant::now();
        let child = Command::new("make")
            .arg(&target)
            .current_dir(&root)
            .env("AOE_RESOURCE_OBSERVE", "1")
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?;
        jobs.push(Job {
            child,
            started,
            root,
            target,
        });
    }
    let begun = Instant::now();
    let mut peaks = BTreeMap::new();
    let mut minimum = u64::MAX;
    let mut failed = false;
    while !jobs.is_empty() {
        let next = Instant::now() + Duration::from_secs(1);
        let available = available_kib(&fs::read_to_string("/proc/meminfo")?)?;
        minimum = minimum.min(available);
        writeln!(
            observations,
            "{:.3}\t{available}",
            begun.elapsed().as_secs_f64()
        )?;
        sample_containers(&mut peaks)?;
        let mut index = 0;
        while index < jobs.len() {
            if let Some(status) = jobs[index].child.try_wait()? {
                let job = jobs.remove(index);
                failed |= !status.success();
                writeln!(
                    results,
                    "{}\t{}\t{:.3}\t{}",
                    job.root.display(),
                    job.target,
                    job.started.elapsed().as_secs_f64(),
                    status.code().unwrap_or(128)
                )?;
            } else {
                index += 1;
            }
        }
        if !jobs.is_empty() {
            thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    }
    let mut containers = File::create(directory.join("container-peaks.tsv"))?;
    writeln!(containers, "container\tworktree\tobserved_peak_bytes")?;
    for (id, (root, bytes)) in peaks {
        writeln!(containers, "{id}\t{root}\t{bytes}")?;
    }
    writeln!(context_file, "minimum MemAvailable: {minimum} KiB")?;
    context(&mut context_file)?;
    println!(
        "measurement: {} (minimum MemAvailable {minimum} KiB)",
        directory.display()
    );
    if failed {
        Err("a measured make job failed; see jobs.tsv and its log".into())
    } else {
        Ok(())
    }
}
