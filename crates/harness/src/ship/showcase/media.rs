//! The side-effecting half of `make showcase`: speech requests, the recording
//! container and the ffmpeg mix. The OpenRouter key is read from the
//! environment, written only to a private request-header file for curl, and
//! deleted with the work directory.
use super::{
    Scene, Segment, Storyboard, TTS_MODEL, Timings, check_duration, durations, measured_duration,
    resolve_out, track,
};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Fixed OpenRouter speech endpoint.
const TTS_URL: &str = "https://openrouter.ai/api/v1/audio/speech";
const RECORDER: &str = include_str!("record.mjs");
pub(crate) const DEFAULT_OUT: &str = ".cache/showcase/showcase.webm";
const STORYBOARD_LIMIT: u64 = 256 * 1024;

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

/// Voice each narrated scene; returns each voice-over's length.
fn narrate(work: &Path, board: &Storyboard) -> Result<Vec<Option<u64>>> {
    if board.scenes.iter().all(|s| s.narration().is_none()) {
        return Ok(vec![None; board.scenes.len()]);
    }
    let key = credential("OPENROUTER_API_KEY").ok_or(
        "the storyboard has narration, but OPENROUTER_API_KEY is neither set nor in the keyring (docs/credentials.md)",
    )?;
    let headers = work.join("headers");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&headers)?
        .write_all(
            format!("Authorization: Bearer {key}\nContent-Type: application/json\n").as_bytes(),
        )?;
    let mut lengths = vec![];
    for (index, scene) in board.scenes.iter().enumerate() {
        let Some(text) = scene.narration() else {
            lengths.push(None);
            continue;
        };
        let request = work.join(format!("tts-{index}.json"));
        let body = serde_json::json!({
            "model": TTS_MODEL, "input": text, "voice": board.voice, "response_format": "pcm",
        });
        fs::write(&request, body.to_string())?;
        let audio = work.join(format!("voice-{index}.pcm"));
        let status = Command::new("curl")
            .args([
                "-sS",
                "--fail-with-body",
                "--max-time",
                "120",
                "-X",
                "POST",
                TTS_URL,
            ])
            .arg("-H")
            .arg(format!("@{}", headers.display()))
            .arg("--data-binary")
            .arg(format!("@{}", request.display()))
            .arg("-o")
            .arg(&audio)
            .status()?;
        if !status.success() {
            let reason = fs::read_to_string(&audio).unwrap_or_default();
            return Err(format!("speech for scene {} failed: {}", index + 1, reason.trim()).into());
        }
        let bytes = fs::metadata(&audio)?.len();
        if bytes == 0 {
            return Err(format!("speech for scene {}: no audio", index + 1).into());
        }
        let seconds = super::pcm_seconds(bytes);
        lengths.push(Some((seconds * 1000.0).ceil() as u64));
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
    fs::write(work.join("plan.json"), serde_json::to_vec(plan)?)?;
    fs::write(work.join("record.mjs"), RECORDER)?;
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
        "-y".into(),
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
        list.push_str(&format!("file '{}'\n", file.display()));
    }
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    docker(&[work], tools, &args)?;
    let concat = work.join("segments.txt");
    fs::write(&concat, list)?;
    let narration = work.join("narration.wav");
    docker(
        &[work],
        tools,
        &[
            "ffmpeg",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            path(&concat),
            path(&narration),
        ],
    )?;
    let out_dir = out.parent().ok_or("the output has no directory")?;
    docker(
        &[work, out_dir],
        tools,
        &[
            "ffmpeg",
            "-loglevel",
            "error",
            "-y",
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
fn inputs(root: &Path) -> Result<(Storyboard, PathBuf)> {
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
        out.display()
    );
    Ok(())
}

pub(crate) fn make(root: &Path) -> Result<PathBuf> {
    let (board, out) = inputs(root)?;
    let tools = env("AOE_SHIP_TOOLS_IMAGE")?;
    let work = root.join(format!(".cache/tmp/showcase-{}", std::process::id()));
    fs::create_dir_all(&work)?;
    let result = (|| -> Result<PathBuf> {
        let voices = narrate(&work, &board)?;
        let _ = fs::remove_file(work.join("headers"));
        let planned = durations(&board, &voices);
        check_duration(&planned)?;
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
        let (silent, timings) = record(root, &work, &plan)?;
        let measured = match measured_duration(&timings, board.scenes.len()) {
            Ok(measured) => measured,
            Err(error) => {
                if error.starts_with("the recorded showcase runs ") {
                    let _ = fs::remove_file(&out);
                }
                return Err(error.into());
            }
        };
        fs::create_dir_all(out.parent().ok_or("the output has no directory")?)?;
        if voices.iter().any(Option::is_some) {
            mix(&work, &tools, &track(&timings, &voices)?, &silent, &out)?;
        } else {
            fs::copy(&silent, &out)?;
        }
        let seconds = measured / 1000;
        println!(
            "showcase: {} ({seconds}s, {} scene(s){})",
            out.display(),
            board.scenes.len(),
            if voices.iter().any(Option::is_some) {
                ", voiced"
            } else {
                ""
            }
        );
        Ok(out.clone())
    })();
    let _ = fs::remove_dir_all(&work);
    result
}
