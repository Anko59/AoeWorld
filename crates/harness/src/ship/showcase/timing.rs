//! Scene timing: planned lengths before the take, measured lengths after it,
//! and the narration track aligned with what was actually recorded.
use super::super::describe::Level;
use super::Storyboard;
use serde::Deserialize;

/// Planned scene lengths: the natural time, stretched to cover the voice.
pub(crate) fn durations(board: &Storyboard, voices: &[Option<u64>]) -> Vec<u64> {
    board
        .scenes
        .iter()
        .enumerate()
        .map(|(i, scene)| {
            let voice = voices.get(i).copied().flatten();
            scene
                .natural_ms()
                .max(voice.map_or(0, |v| v.saturating_add(600)))
        })
        .collect()
}

pub(crate) fn check_duration(level: Level, durations: &[u64]) -> Result<(), String> {
    let limit = level
        .video_limit()
        .ok_or_else(|| format!("{} PRs have no showcase video", level.name()))?;
    let total = durations
        .iter()
        .fold(0_u64, |sum, duration| sum.saturating_add(*duration));
    if total > u64::from(limit) * 1000 {
        return Err(format!(
            "the storyboard runs {:.1}s; a {} PR allows {limit}s: cut it",
            total as f64 / 1000.0,
            level.name()
        ));
    }
    Ok(())
}

pub(crate) fn measured_duration(
    level: Level,
    timings: &Timings,
    scene_count: usize,
) -> Result<u64, String> {
    if timings.scenes_ms.len() != scene_count {
        return Err(format!(
            "the recorder measured {} scene(s) for a storyboard of {scene_count}",
            timings.scenes_ms.len()
        ));
    }
    let total = timings
        .scenes_ms
        .iter()
        .fold(timings.lead_in_ms, |sum, duration| {
            sum.saturating_add(*duration)
        });
    let limit = level
        .video_limit()
        .ok_or_else(|| format!("{} PRs have no showcase video", level.name()))?;
    if total > u64::from(limit) * 1000 {
        return Err(format!(
            "the recorded showcase runs {:.1}s; a {} PR allows {limit}s: cut it",
            total as f64 / 1000.0,
            level.name()
        ));
    }
    Ok(total)
}

/// What the recorder measured: blank video before the first scene, then the
/// actual length of each scene (a slow page can overrun its plan).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Timings {
    pub(crate) lead_in_ms: u64,
    pub(crate) scenes_ms: Vec<u64>,
}

/// One piece of the narration track: a scene's voice-over (by scene index)
/// or silence, held for `ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) voice: Option<usize>,
    pub(crate) ms: u64,
}

/// The narration track that lines up with the recorded video.
pub(crate) fn track(timings: &Timings, voices: &[Option<u64>]) -> Result<Vec<Segment>, String> {
    if timings.scenes_ms.len() != voices.len() {
        return Err(format!(
            "the recorder measured {} scene(s) for a storyboard of {}",
            timings.scenes_ms.len(),
            voices.len()
        ));
    }
    let mut segments = vec![];
    if timings.lead_in_ms > 0 {
        segments.push(Segment {
            voice: None,
            ms: timings.lead_in_ms,
        });
    }
    for (index, (ms, voice)) in timings.scenes_ms.iter().zip(voices).enumerate() {
        segments.push(Segment {
            voice: voice.map(|_| index),
            ms: *ms,
        });
    }
    Ok(segments)
}
