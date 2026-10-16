//! Content-addressed narration reused across scenes and takes. Credentials
//! are looked up only on a cache miss; only complete valid PCM is published.
mod cache;
#[cfg(test)]
mod tests;

use super::workdir::create_new;
use super::{Storyboard, TTS_MODEL, TTS_PCM_BUDGET, check_duration, durations};
use crate::ship::describe::Level;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const TTS_URL: &str = "https://openrouter.ai/api/v1/audio/speech";
#[cfg(test)]
const TTS_RESPONSE_LIMIT: u64 = 32 * 1024 * 1024;
const TTS_ERROR_LIMIT: u64 = 64 * 1024;

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

pub(crate) fn tts_command(request: &Path) -> Command {
    let mut command = Command::new("curl");
    command
        .args(["-q", "-sS", "--fail-with-body", "--max-time", "120"])
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

/// Voice each narrated scene; returns each voice-over's length.
pub(crate) fn narrate(
    root: &Path,
    work: &Path,
    board: &Storyboard,
    level: Level,
) -> Result<Vec<Option<u64>>> {
    if board.scenes.iter().all(|s| s.narration().is_none()) {
        return Ok(vec![None; board.scenes.len()]);
    }
    let mut lengths = vec![];
    let mut total_bytes = 0_u64;
    for (index, scene) in board.scenes.iter().enumerate() {
        let Some(text) = scene.narration() else {
            lengths.push(None);
            continue;
        };
        let audio = work.join(format!("voice-{index}.pcm"));
        let remaining = TTS_PCM_BUDGET.saturating_sub(total_bytes);
        if remaining == 0 {
            return Err(format!(
                "narration exceeds the {TTS_PCM_BUDGET}-byte aggregate PCM budget"
            )
            .into());
        }
        let entry = cache::entry(root, TTS_MODEL, &board.voice, text)?;
        let bytes = if let Some(bytes) = cache::load(&entry, &audio, remaining)? {
            println!("showcase-tts: scene {} cache hit", index + 1);
            bytes
        } else {
            println!("showcase-tts: scene {} cache miss", index + 1);
            let key = credential("OPENROUTER_API_KEY").ok_or(
                "the storyboard has narration, but OPENROUTER_API_KEY is neither set nor in the keyring (docs/credentials.md)",
            )?;
            let request = work.join(format!("tts-{index}.json"));
            let body = serde_json::json!({
                "model": TTS_MODEL, "input": text, "voice": board.voice, "response_format": "pcm",
            });
            create_new(&request)?.write_all(body.to_string().as_bytes())?;
            let mut child = tts_command(&request).spawn()?;
            child
                .stdin
                .take()
                .ok_or("curl stdin was not piped")?
                .write_all(
                    format!("Authorization: Bearer {key}\nContent-Type: application/json\n")
                        .as_bytes(),
                )?;
            let bytes = save_tts_response_limited(&mut child, &audio, remaining)
                .map_err(|error| format!("speech for scene {} failed: {error}", index + 1))?;
            if bytes == 0 {
                return Err(format!("speech for scene {}: no audio", index + 1).into());
            }
            if bytes % 2 != 0 {
                return Err(format!(
                    "speech for scene {}: invalid PCM (odd byte count)",
                    index + 1
                )
                .into());
            }
            super::media::publish_temp_output(&audio, &entry, work)?;
            bytes
        };
        total_bytes = total_bytes.saturating_add(bytes);
        let seconds = super::pcm_seconds(bytes);
        lengths.push(Some((seconds * 1000.0).ceil() as u64));
        if lengths.len() == index + 1 {
            let planned = durations(board, &lengths);
            if let Err(error) = check_duration(level, &planned) {
                return Err(
                    format!("narration exceeds the showcase duration budget: {error}").into(),
                );
            }
        }
    }
    Ok(lengths)
}
