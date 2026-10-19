//! The side-effecting half of `make showcase`: speech requests, the recording
//! container and the ffmpeg mix. The OpenRouter key is passed to curl over a
//! pipe and never written to disk.
use super::super::describe::Level;
use super::bridge::Bridge;
use super::browser::{Plan, plan};
use super::mounts::bind;
use super::origins::Origins;
use super::workdir::{create_new, write_recorder};
use super::{
    Segment, ShowcaseOutput, Storyboard, Timings, check_duration, durations, measured_duration,
    narrate, resolve_out, track,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) const DEFAULT_OUT: &str = ".cache/showcase/showcase.webm";
const STORYBOARD_LIMIT: u64 = 256 * 1024;

fn env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is unset: run through `make showcase`").into())
}

fn image_env(name: &str) -> Result<String> {
    let image = env(name)?;
    super::check::validate_image_value(&image).map_err(|error| format!("{name} {error}"))?;
    Ok(image)
}

fn user() -> String {
    format!("{}:{}", nix::unistd::getuid(), nix::unistd::getgid())
}

/// Run Docker as the caller with unambiguous bind mounts.
fn docker(mounts: &[&Path], image: &str, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new("docker");
    command.args(["run", "--rm", "--user", &user()]);
    for dir in mounts {
        command.arg("--mount").arg(bind(dir, dir, false));
    }
    let output = command.arg(image).args(args).output()?;
    if !output.status.success() {
        return Err(format!(
            "{image} {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(output)
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

pub(crate) fn concat_entry(file: &Path) -> String {
    let escaped = path(file).replace('\'', "'\\''");
    format!("file '{escaped}'\n")
}

/// Record the plan in one Playwright take; returns the silent WebM and timings.
/// The container never has a network. Each origin the browser scenes film gets
/// a bridge forwarding its own Unix socket in the work directory to that app's
/// port alone (bridge.rs), and record.mjs still aborts every request off the
/// origin of the scene on screen.
fn record(root: &Path, work: &Path, plan: &Plan<'_>) -> Result<(PathBuf, Timings)> {
    let browser = image_env("BROWSER_IMAGE")?;
    let modules = super::preparation::modules(root);
    if !modules.join("playwright").is_dir() {
        return Err(format!(
            "{} has no Playwright: run `make browser-deps`",
            modules.display()
        )
        .into());
    }
    // A skirmish start needs its map's raster served: do it before filming.
    super::steps::warm::prepare(plan)?;
    create_new(&work.join("plan.json"))?.write_all(&serde_json::to_vec(plan)?)?;
    write_recorder(work)?;
    let mut bridges = Vec::with_capacity(plan.origins.len());
    for bridged in &plan.origins {
        let (host, port) = bridged.origin.address()?;
        bridges.push(Bridge::start(work, bridged.socket, &host, port)?);
    }
    let status = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--init",
            "--network",
            "none",
            "--ipc",
            "host",
            "--user",
            &user(),
        ])
        .args(["-e", "HOME=/tmp", "-w", path(work), "--mount"])
        .arg(bind(work, work, false))
        .arg("--mount")
        .arg(bind(&modules, &work.join("node_modules"), true))
        .args([browser.as_str(), "node", "record.mjs"])
        .status()?;
    drop(bridges);
    if !status.success() {
        let apps: Vec<&str> = plan
            .origins
            .iter()
            .map(|bridged| bridged.origin.http())
            .collect();
        return Err(if apps.is_empty() {
            "the recorder failed".into()
        } else {
            format!(
                "the recorder failed (is the app up at {}?)",
                apps.join(" and ")
            )
            .into()
        });
    }
    let timings: Timings = serde_json::from_slice(&fs::read(work.join("timings.json"))?)?;
    Ok((work.join("silent.webm"), timings))
}

/// Pad each voice (or silence) to its scene's measured length, then mux.
fn mix(work: &Path, tools: &str, segments: &[Segment], video: &Path, out: &Path) -> Result<()> {
    let mut list = String::new();
    let mut args = vec![
        "ffmpeg".to_owned(),
        "-loglevel".into(),
        "error".into(),
        "-n".into(),
    ];
    for (index, segment) in segments.iter().enumerate() {
        let file = work.join(format!("segment-{index}.wav"));
        let seconds = format!("{:.3}", segment.ms as f64 / 1000.0);
        let voice = segment.voice.map(|v| work.join(format!("voice-{v}.pcm")));
        match &voice {
            Some(voice) => args.extend([
                "-f".into(),
                "s16le".into(),
                "-ar".into(),
                super::PCM_RATE.to_string(),
                "-ac".into(),
                "1".into(),
                "-i".into(),
                path(voice).into(),
            ]),
            None => args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "anullsrc=r=48000:cl=mono".into(),
            ]),
        }
        args.extend(["-map".into(), format!("{index}:a:0")]);
        if voice.is_some() {
            args.extend(["-af".into(), "apad".into()]);
        }
        args.extend([
            "-t".into(),
            seconds,
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "1".into(),
            path(&file).into(),
        ]);
        list.push_str(&concat_entry(&file));
    }
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    docker(&[work], tools, &args)?;
    let concat = work.join("segments.txt");
    create_new(&concat)?.write_all(list.as_bytes())?;
    let narration = work.join("narration.wav");
    docker(
        &[work],
        tools,
        &[
            "ffmpeg",
            "-loglevel",
            "error",
            "-n",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            path(&concat),
            path(&narration),
        ],
    )?;
    docker(
        &[work],
        tools,
        &[
            "ffmpeg",
            "-loglevel",
            "error",
            "-n",
            "-i",
            path(video),
            "-i",
            path(&narration),
            "-c:v",
            "copy",
            "-c:a",
            "libopus",
            "-shortest",
            path(out),
        ],
    )?;
    Ok(())
}

/// `make showcase`: SHOWCASE_STORYBOARD in, SHOWCASE_OUT (default
/// `.cache/showcase/showcase.webm`) out, ready for SHIP_VIDEO.
fn inputs(root: &Path) -> Result<(Storyboard, ShowcaseOutput, Level)> {
    super::check::validate_manifest_root(root)?;
    let level = super::check::showcase_level(root)?;
    let storyboard = env("SHOWCASE_STORYBOARD")?;
    let mut board = Storyboard::parse(&read_storyboard(Path::new(&storyboard))?)?;
    super::steps::resolve_bookmarks(&mut board, root)?;
    super::check::check_voice_requirement(level, &board)?;
    let requested_out = std::env::var("SHOWCASE_OUT")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_OUT.into());
    let out = resolve_out(root, &requested_out)?;
    let planned = durations(&board, &vec![None; board.scenes.len()]);
    check_duration(level, &planned)?;
    let mut validation = plan(&board, &planned, &Origins::from_env(root, &board)?)?;
    super::capture::apply(root, &mut validation)?;
    Ok((board, out, level))
}

pub(crate) fn read_storyboard(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(format!("storyboard {} must be a regular file", path.display()).into());
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(STORYBOARD_LIMIT) as usize);
    fs::File::open(path)?
        .take(STORYBOARD_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > STORYBOARD_LIMIT {
        return Err(format!("storyboard exceeds the {STORYBOARD_LIMIT}-byte limit").into());
    }
    String::from_utf8(bytes).map_err(|error| format!("storyboard is not UTF-8: {error}").into())
}

/// Validate the storyboard, output path and unvoiced duration without starting
/// a Docker build, recorder or speech request.
pub(crate) fn check(root: &Path) -> Result<()> {
    let (board, out, _) = inputs(root)?;
    println!(
        "showcase-check: {} scene(s), output {}",
        board.scenes.len(),
        out.path.display()
    );
    Ok(())
}

pub(crate) fn make(root: &Path) -> Result<PathBuf> {
    let (board, out, level) = inputs(root)?;
    let _browser = image_env("BROWSER_IMAGE")?;
    let tools = image_env("SHIP_TOOLS_IMAGE")?;
    let work = super::workdir::create_work_dir(root)?;
    let work_path = work.path();
    (|| -> Result<PathBuf> {
        let phase = Instant::now();
        let voices = narrate(root, work_path, &board, level)?;
        eprintln!(
            "showcase-phase narration_ms {}",
            phase.elapsed().as_millis()
        );
        let planned = durations(&board, &voices);
        check_duration(level, &planned)?;
        let make = env("MAKE")?;
        let phase = Instant::now();
        let reuse = super::preparation::ready(root, &super::preparation::modules(root));
        let dependencies = if reuse {
            "browser-tools"
        } else {
            "browser-deps"
        };
        println!(
            "showcase-deps: cache {}",
            if reuse { "hit" } else { "miss" }
        );
        let status = Command::new(make)
            .args(["--no-print-directory", "ship-tools", dependencies])
            .status()?;
        if !status.success() {
            return Err("building showcase images failed".into());
        }
        eprintln!(
            "showcase-phase image_dependencies_ms {}",
            phase.elapsed().as_millis()
        );
        let mut plan = plan(&board, &planned, &Origins::from_env(root, &board)?)?;
        super::capture::apply(root, &mut plan)?;
        let phase = Instant::now();
        let (silent, timings) = record(root, work_path, &plan)?;
        eprintln!("showcase-phase recorder_ms {}", phase.elapsed().as_millis());
        super::capture::verify(work_path, &plan)?;
        let measured = measured_duration(level, &timings, board.scenes.len())?;
        let encoded = work_path.join("showcase.webm");
        let phase = Instant::now();
        let voiced = voices.iter().any(Option::is_some);
        let publish = if voiced {
            mix(
                work_path,
                &tools,
                &track(&timings, &voices)?,
                &silent,
                &encoded,
            )
        } else {
            let mut source = fs::File::open(&silent)?;
            let mut target = create_new(&encoded)?;
            std::io::copy(&mut source, &mut target)
                .map(|_| ())
                .map_err(Into::into)
        };
        publish?;
        eprintln!(
            "showcase-phase ffmpeg_ms {}",
            if voiced {
                phase.elapsed().as_millis()
            } else {
                0
            }
        );
        publish_temp_output(&encoded, &out, work_path)?;
        let seconds = measured / 1000;
        println!(
            "showcase: {} ({seconds}s, {} scene(s){})",
            out.path.display(),
            board.scenes.len(),
            if voices.iter().any(Option::is_some) {
                ", voiced"
            } else {
                ""
            }
        );
        Ok(out.path.clone())
    })()
}

pub(crate) fn publish_temp_output(source: &Path, out: &ShowcaseOutput, work: &Path) -> Result<()> {
    let seed = tempfile::Builder::new()
        .prefix("publish-")
        .tempfile_in(work)?;
    let temp_name = seed
        .path()
        .file_name()
        .ok_or("temporary publish filename is missing")?;
    let temp = std::ffi::CString::new(temp_name.as_encoded_bytes())?;
    drop(seed);
    let fd = nix::fcntl::openat(
        &out.directory,
        temp.as_c_str(),
        nix::fcntl::OFlag::O_CREAT
            | nix::fcntl::OFlag::O_EXCL
            | nix::fcntl::OFlag::O_WRONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )?;
    let mut target = fs::File::from(fd);
    let copy_result = (|| -> Result<()> {
        let mut input = fs::File::open(source)?;
        std::io::copy(&mut input, &mut target)?;
        target.sync_all()?;
        Ok(())
    })();
    drop(target);
    let publish_result = copy_result.and_then(|()| {
        nix::fcntl::renameat(
            &out.directory,
            temp.as_c_str(),
            &out.directory,
            out.name.as_c_str(),
        )
        .map_err(Into::into)
    });
    if let Err(error) = publish_result {
        let _ = nix::unistd::unlinkat(
            &out.directory,
            temp.as_c_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        );
        return Err(error);
    }
    Ok(())
}
