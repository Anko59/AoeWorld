//! The side-effecting half of `make showcase`: speech requests, the recording
//! container and the ffmpeg mix. The OpenRouter key is passed to curl over a
//! pipe and never written to disk.
use super::workdir::create_new;
use super::{
    Scene, Segment, ShowcaseOutput, Storyboard, TTS_MODEL, TTS_PCM_BUDGET, Timings, check_duration,
    durations, measured_duration, resolve_out, track,
};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Fixed OpenRouter speech endpoint.
const TTS_URL: &str = "https://openrouter.ai/api/v1/audio/speech";
const RECORDER: &str = include_str!("record.mjs");
pub(crate) const DEFAULT_OUT: &str = ".cache/showcase/showcase.webm";
const STORYBOARD_LIMIT: u64 = 256 * 1024;
#[cfg(test)]
const TTS_RESPONSE_LIMIT: u64 = 32 * 1024 * 1024;
const TTS_ERROR_LIMIT: u64 = 64 * 1024;

#[derive(Serialize)]
struct Plan<'a> {
    title: &'a str,
    scenes: Vec<Planned<'a>>,
}

#[derive(Serialize)]
struct Planned<'a> {
    #[serde(flatten)]
    scene: &'a Scene,
    duration_ms: u64,
}

fn env(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is unset: run through `make showcase`").into())
}

/// A secret from the environment, else from the keyring, where every
/// credential of this project lives (docs/credentials.md).
fn credential(name: &str) -> Option<String> {
    if let Some(value) = std::env::var(name).ok().filter(|v| !v.is_empty()) {
        return Some(value);
    }
    let output = Command::new("secret-tool")
        .args(["lookup", "service", "codex-api", "name", name])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !value.is_empty()).then_some(value)
}

fn user() -> String {
    format!("{}:{}", nix::unistd::getuid(), nix::unistd::getgid())
}

/// `docker run --rm --user <me> -v <dir>:<dir>… <image> <args…>`.
fn docker(mounts: &[&Path], image: &str, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new("docker");
    command.args(["run", "--rm", "--user", &user()]);
    for dir in mounts {
        command.arg("-v").arg(format!("{0}:{0}", dir.display()));
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

pub(crate) fn tts_command(request: &Path) -> Command {
    let mut command = Command::new("curl");
    command
        .args(["-sS", "--fail-with-body", "--max-time", "120"])
        .args(["-X", "POST", TTS_URL, "-H", "@-", "--data-binary"])
        .arg(format!("@{}", request.display()))
        .stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command
}

#[cfg(test)]
pub(crate) fn save_tts_response(child: &mut std::process::Child, audio: &Path) -> Result<u64> {
    save_tts_response_limited(child, audio, TTS_RESPONSE_LIMIT)
}

fn save_tts_response_limited(
    child: &mut std::process::Child,
    audio: &Path,
    limit: u64,
) -> Result<u64> {
    let mut stdout = child.stdout.take().ok_or("curl stdout was not piped")?;
    let mut limited = (&mut stdout).take(limit + 1);
    let mut file = create_new(audio)?;
    let mut total = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = limited.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(total) as usize;
        let accepted = count.min(remaining);
        file.write_all(&buffer[..accepted])?;
        total += accepted as u64;
        if accepted != count {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("speech response exceeds the {limit}-byte limit").into());
        }
    }
    let status = child.wait()?;
    if !status.success() {
        let mut reason = String::new();
        let file = fs::File::open(audio)?;
        file.take(TTS_ERROR_LIMIT).read_to_string(&mut reason)?;
        return Err(format!("speech request failed: {}", reason.trim()).into());
    }
    Ok(total)
}

pub(crate) fn concat_entry(file: &Path) -> String {
    let escaped = path(file).replace('\'', "'\\''");
    format!("file '{escaped}'\n")
}

/// Voice each narrated scene; returns each voice-over's length.
pub(crate) fn narrate(work: &Path, board: &Storyboard) -> Result<Vec<Option<u64>>> {
    if board.scenes.iter().all(|s| s.narration().is_none()) {
        return Ok(vec![None; board.scenes.len()]);
    }
    let key = credential("OPENROUTER_API_KEY").ok_or(
        "the storyboard has narration, but OPENROUTER_API_KEY is neither set nor in the keyring (docs/credentials.md)",
    )?;
    let mut lengths = vec![];
    let mut total_bytes = 0_u64;
    for (index, scene) in board.scenes.iter().enumerate() {
        let Some(text) = scene.narration() else {
            lengths.push(None);
            continue;
        };
        let request = work.join(format!("tts-{index}.json"));
        let body = serde_json::json!({
            "model": TTS_MODEL, "input": text, "voice": board.voice, "response_format": "pcm",
        });
        create_new(&request)?.write_all(body.to_string().as_bytes())?;
        let audio = work.join(format!("voice-{index}.pcm"));
        let remaining = TTS_PCM_BUDGET.saturating_sub(total_bytes);
        if remaining == 0 {
            return Err(format!(
                "narration exceeds the {TTS_PCM_BUDGET}-byte aggregate PCM budget"
            )
            .into());
        }
        let mut child = tts_command(&request).spawn()?;
        child
            .stdin
            .take()
            .ok_or("curl stdin was not piped")?
            .write_all(
                format!("Authorization: Bearer {key}\nContent-Type: application/json\n").as_bytes(),
            )?;
        let bytes = save_tts_response_limited(&mut child, &audio, remaining)
            .map_err(|error| format!("speech for scene {} failed: {error}", index + 1))?;
        if bytes == 0 {
            return Err(format!("speech for scene {}: no audio", index + 1).into());
        }
        total_bytes = total_bytes.saturating_add(bytes);
        let seconds = super::pcm_seconds(bytes);
        lengths.push(Some((seconds * 1000.0).ceil() as u64));
        if lengths.len() == index + 1 {
            let planned = durations(board, &lengths);
            if let Err(error) = check_duration(&planned) {
                return Err(
                    format!("narration exceeds the showcase duration budget: {error}").into(),
                );
            }
        }
    }
    Ok(lengths)
}

/// Record the plan in one Playwright take; returns the silent WebM and timings.
fn record(root: &Path, work: &Path, plan: &Plan<'_>) -> Result<(PathBuf, Timings)> {
    let browser = env("AOE_BROWSER_IMAGE")?;
    let modules = std::env::var_os("AOE_SHOWCASE_NODE_MODULES")
        .map_or_else(|| root.join("browser/node_modules"), PathBuf::from);
    if !modules.join("playwright").is_dir() {
        return Err(format!(
            "{} has no Playwright: run `make browser-deps`",
            modules.display()
        )
        .into());
    }
    create_new(&work.join("plan.json"))?.write_all(&serde_json::to_vec(plan)?)?;
    create_new(&work.join("record.mjs"))?.write_all(RECORDER.as_bytes())?;
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
        .args(["-e", "HOME=/tmp", "-w", path(work), "-v"])
        .arg(format!("{0}:{0}", work.display()))
        .arg("-v")
        .arg(format!(
            "{}:{}/node_modules:ro",
            modules.display(),
            work.display()
        ))
        .args([browser.as_str(), "node", "record.mjs"])
        .status()?;
    if !status.success() {
        return Err("the recorder failed".into());
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
fn inputs(root: &Path) -> Result<(Storyboard, ShowcaseOutput)> {
    let storyboard = env("SHOWCASE_STORYBOARD")?;
    let board = Storyboard::parse(&read_storyboard(Path::new(&storyboard))?)?;
    let requested_out = std::env::var("SHOWCASE_OUT")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_OUT.into());
    let out = resolve_out(root, &requested_out)?;
    check_duration(&durations(&board, &vec![None; board.scenes.len()]))?;
    Ok((board, out))
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
    let (board, out) = inputs(root)?;
    println!(
        "showcase-check: {} scene(s), output {}",
        board.scenes.len(),
        out.path.display()
    );
    Ok(())
}

pub(crate) fn make(root: &Path) -> Result<PathBuf> {
    let (board, out) = inputs(root)?;
    let tools = env("AOE_SHIP_TOOLS_IMAGE")?;
    let work = super::workdir::create_work_dir(root)?;
    let work_path = work.path();
    (|| -> Result<PathBuf> {
        let voices = narrate(work_path, &board)?;
        let planned = durations(&board, &voices);
        check_duration(&planned)?;
        let make = env("MAKE")?;
        let status = Command::new(make)
            .args(["--no-print-directory", "ship-tools", "browser-deps"])
            .status()?;
        if !status.success() {
            return Err("building showcase images failed".into());
        }
        let plan = Plan {
            title: &board.title,
            scenes: board
                .scenes
                .iter()
                .zip(&planned)
                .map(|(scene, ms)| Planned {
                    scene,
                    duration_ms: *ms,
                })
                .collect(),
        };
        let (silent, timings) = record(root, work_path, &plan)?;
        let measured = measured_duration(&timings, board.scenes.len())?;
        let encoded = work_path.join("showcase.webm");
        let publish = if voices.iter().any(Option::is_some) {
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
