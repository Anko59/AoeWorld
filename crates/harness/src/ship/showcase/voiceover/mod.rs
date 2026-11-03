//! `make showcase-narrate`: a voice-over for a video recorded some other way
//! (a hand-written Playwright script, an ffmpeg cut). Each line is voiced by the
//! same TTS path as showcase narration (same model, cache and credential
//! handling), laid at its timestamp on a silent 24 kHz timeline as long as the
//! video, and muxed in as the video's only audio track, so `make describe-check`
//! sees real speech. Lines may neither overlap nor run past the video's end.
use super::media::{docker, env, image_env, path, publish_temp_output, read_bounded};
use super::workdir::{create_new, create_work_dir};
use super::{MAX_SECONDS, PCM_RATE, TTS_MAX_CHARS, TTS_PCM_BUDGET, narration::speech, resolve_out};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const MAX_LINES: usize = 40;
const LINES_LIMIT: u64 = 64 * 1024;
const VIDEO_LIMIT: u64 = 512 * 1024 * 1024;
const DEFAULT_VOICE: &str = "Kore";

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct VoiceLine {
    /// Seconds from the start of the video at which the voice begins.
    pub(crate) at: f64,
    pub(crate) text: String,
}

/// The strictly parsed, ordered lines of NARRATE_LINES.
pub(crate) fn parse_lines(text: &str) -> Result<Vec<VoiceLine>> {
    let lines: Vec<VoiceLine> = serde_json::from_str(text).map_err(|error| {
        format!("narration lines must be [{{\"at\": seconds, \"text\": \"...\"}}]: {error}")
    })?;
    if lines.is_empty() || lines.len() > MAX_LINES {
        return Err(format!(
            "narration needs 1 to {MAX_LINES} lines, got {}",
            lines.len()
        )
        .into());
    }
    let mut previous = 0.0_f64;
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        if !line.at.is_finite() || line.at < 0.0 || line.at > MAX_SECONDS as f64 {
            return Err(format!("line {number}: `at` must be 0 to {MAX_SECONDS} seconds").into());
        }
        if line.at < previous {
            return Err(format!("line {number}: lines must be in ascending `at` order").into());
        }
        previous = line.at;
        let length = line.text.trim().chars().count();
        if length == 0 || length > TTS_MAX_CHARS {
            return Err(
                format!("line {number}: text must be 1 to {TTS_MAX_CHARS} characters").into(),
            );
        }
    }
    Ok(lines)
}

/// Seconds of the first input in ffmpeg's banner (`-i <video>`); `None` when
/// the container records no duration (remux it: `ffmpeg -i in -c copy out`).
pub(crate) fn probe_seconds(banner: &str) -> Option<f64> {
    let first = banner.split("Input #").nth(1)?;
    let stamp = first
        .lines()
        .find(|line| line.trim_start().starts_with("Duration:"))?
        .split_once("Duration:")?
        .1
        .split(',')
        .next()?;
    let mut parts = stamp.trim().split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    (first.contains(": Video:") && total > 0.0).then_some(total)
}

fn samples(seconds: f64) -> u64 {
    (seconds * f64::from(PCM_RATE)).round() as u64
}

/// Bytes of silence before each voice and after the last, so that the voices
/// land on their timestamps in a track exactly `duration` seconds long.
pub(crate) fn silences(
    lines: &[VoiceLine],
    voice_bytes: &[u64],
    duration: f64,
) -> Result<Vec<u64>> {
    let total = samples(duration);
    let mut cursor = 0_u64;
    let mut gaps = Vec::with_capacity(lines.len() + 1);
    for (index, (line, bytes)) in lines.iter().zip(voice_bytes).enumerate() {
        let start = samples(line.at);
        let end = start + bytes / 2;
        if start < cursor {
            return Err(format!(
                "line {} starts at {:.2}s while line {} is still speaking until {:.2}s",
                index + 1,
                line.at,
                index,
                cursor as f64 / f64::from(PCM_RATE)
            )
            .into());
        }
        if end > total {
            return Err(format!(
                "line {} speaks until {:.2}s, past the {duration:.2}s video",
                index + 1,
                end as f64 / f64::from(PCM_RATE)
            )
            .into());
        }
        gaps.push((start - cursor) * 2);
        cursor = end;
    }
    gaps.push((total - cursor) * 2);
    Ok(gaps)
}

fn write_silence(out: &mut fs::File, mut bytes: u64) -> std::io::Result<()> {
    let zeros = [0_u8; 8192];
    while bytes > 0 {
        let chunk = bytes.min(zeros.len() as u64) as usize;
        out.write_all(&zeros[..chunk])?;
        bytes -= chunk as u64;
    }
    Ok(())
}

/// `make showcase-narrate NARRATE_VIDEO=<in> NARRATE_LINES=<json> SHOWCASE_OUT=<out>`.
pub(crate) fn make(root: &Path) -> Result<PathBuf> {
    super::check::validate_manifest_root(root)?;
    let video = env("NARRATE_VIDEO")?;
    let lines_file = env("NARRATE_LINES")?;
    let out = resolve_out(root, &env("SHOWCASE_OUT")?)?;
    let voice = std::env::var("NARRATE_VOICE")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_VOICE.into());
    let lines = parse_lines(&read_bounded(
        Path::new(&lines_file),
        "narration lines",
        LINES_LIMIT,
    )?)?;
    let tools = image_env("SHIP_TOOLS_IMAGE")?;
    let source = Path::new(&video);
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.is_file() || metadata.len() > VIDEO_LIMIT {
        return Err(
            format!("{video} must be a regular file of at most {VIDEO_LIMIT} bytes").into(),
        );
    }
    let status = Command::new(env("MAKE")?)
        .args(["--no-print-directory", "ship-tools"])
        .status()?;
    if !status.success() {
        return Err("building the ship-tools image failed".into());
    }
    let work = create_work_dir(root)?;
    let work = work.path();
    let input = work.join("input.webm");
    std::io::copy(
        &mut fs::File::open(source)?.take(VIDEO_LIMIT),
        &mut create_new(&input)?,
    )?;
    // ffmpeg exits nonzero without an output file; the banner is what we want.
    crate::resources::policy::check()?;
    let banner = Command::new("docker")
        .arg("run")
        .args(crate::jobs::cap_args())
        .args(["--rm", "--user"])
        .arg(format!(
            "{}:{}",
            nix::unistd::getuid(),
            nix::unistd::getgid()
        ))
        .arg("--mount")
        .arg(super::mounts::bind(work, work, true))
        .arg(&tools)
        .args(["ffmpeg", "-hide_banner", "-i", path(&input)])
        .output()?;
    let banner = String::from_utf8_lossy(&banner.stderr);
    let duration = probe_seconds(&banner).ok_or(
        "cannot read the video's duration (remux it with `ffmpeg -i in -c copy out.webm`)",
    )?;
    if let Some(late) = lines.iter().position(|line| line.at >= duration) {
        return Err(format!(
            "line {} starts at or past the {duration:.2}s video",
            late + 1
        )
        .into());
    }
    let mut lengths = Vec::with_capacity(lines.len());
    let mut total = 0_u64;
    for (index, line) in lines.iter().enumerate() {
        let remaining = TTS_PCM_BUDGET.saturating_sub(total);
        if remaining == 0 {
            return Err(format!(
                "narration exceeds the {TTS_PCM_BUDGET}-byte aggregate PCM budget"
            )
            .into());
        }
        let bytes = speech(
            root,
            work,
            &format!("line-{index}"),
            &format!("line {}", index + 1),
            &voice,
            line.text.trim(),
            remaining,
        )?;
        total += bytes;
        lengths.push(bytes);
    }
    let gaps = silences(&lines, &lengths, duration)?;
    let timeline = work.join("timeline.pcm");
    let mut track = create_new(&timeline)?;
    for (index, gap) in gaps.iter().enumerate() {
        write_silence(&mut track, *gap)?;
        if index < lines.len() {
            let mut voice = fs::File::open(work.join(format!("line-{index}.pcm")))?;
            std::io::copy(&mut voice, &mut track)?;
        }
    }
    track.sync_all()?;
    drop(track);
    let encoded = work.join("narrated.webm");
    let rate = PCM_RATE.to_string();
    docker(
        &[work],
        &tools,
        &[
            "ffmpeg",
            "-loglevel",
            "error",
            "-n",
            "-i",
            path(&input),
            "-f",
            "s16le",
            "-ar",
            &rate,
            "-ac",
            "1",
            "-i",
            path(&timeline),
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "libopus",
            "-ar",
            "48000",
            "-shortest",
            path(&encoded),
        ],
    )?;
    publish_temp_output(&encoded, &out, work)?;
    println!(
        "showcase-narrate: {} ({duration:.0}s, {} voiced line(s))",
        out.path.display(),
        lines.len()
    );
    Ok(out.path.clone())
}
