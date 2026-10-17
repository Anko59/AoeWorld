//! Preset maps (`AOE_PRESET_MAPS`): the requests a harness-owned server
//! prepares once at boot, and the reuse of any stored package that is what
//! preparing a request again would produce. Without the file, every request
//! is prepared by the worker, as before. The package directory's owner is
//! responsible for it never holding packages of older map code.
use super::{CreationRequest, JobState, PreparationPlan, start, status};
use crate::{AppState, map_worker};
use aoe_map::{MapPackage, MapRequest};
use std::{path::Path, sync::atomic::Ordering, time::Duration};

const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_PRESETS: usize = 8;
const POLL: Duration = Duration::from_millis(250);

/// The stored package that already answers `request` under `preparation`.
pub(super) async fn reusable(
    state: &AppState,
    request: MapRequest,
    preparation: &PreparationPlan,
) -> Option<MapPackage> {
    if !state.reuse_prepared_maps.load(Ordering::SeqCst) {
        return None;
    }
    let packages = state.map_packages.read().await;
    packages
        .values()
        .find(|package| map_worker::reusable(package, request, preparation))
        .cloned()
}

/// The requests of a preset file: a bounded JSON array of creation requests.
fn read_presets(file: &Path) -> Result<Vec<CreationRequest>, String> {
    let length = std::fs::metadata(file)
        .map_err(|error| format!("AOE_PRESET_MAPS {}: {error}", file.display()))?
        .len();
    if length > MAX_FILE_BYTES {
        return Err(format!(
            "AOE_PRESET_MAPS {}: over {MAX_FILE_BYTES} bytes",
            file.display()
        ));
    }
    let bytes = std::fs::read(file)
        .map_err(|error| format!("AOE_PRESET_MAPS {}: {error}", file.display()))?;
    let presets: Vec<CreationRequest> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("AOE_PRESET_MAPS {}: {error}", file.display()))?;
    if presets.len() > MAX_PRESETS {
        return Err(format!(
            "AOE_PRESET_MAPS {}: more than {MAX_PRESETS} maps",
            file.display()
        ));
    }
    Ok(presets)
}

impl AppState {
    /// Turn reuse on and prepare each preset of `file`, one after the other,
    /// in the background (a preset a stored package answers completes at
    /// once). Progress is the ordinary job list. Returns how many were queued.
    pub async fn prepare_preset_maps(&self, file: &Path) -> Result<usize, String> {
        let presets = read_presets(file)?;
        self.reuse_prepared_maps.store(true, Ordering::SeqCst);
        let count = presets.len();
        let state = self.clone();
        tokio::spawn(async move {
            for preset in presets {
                let job = match start(&state, preset, None).await {
                    Ok(job) => job,
                    Err(error) => {
                        tracing::error!(%error, "preset map refused");
                        continue;
                    }
                };
                loop {
                    match status(&state, job.id).await.map(|job| job.state) {
                        Some(JobState::Completed) => {
                            tracing::info!(job = job.id, "preset map ready");
                            break;
                        }
                        Some(JobState::Failed | JobState::Cancelled) | None => {
                            tracing::error!(job = job.id, "preset map failed");
                            break;
                        }
                        _ => tokio::time::sleep(POLL).await,
                    }
                }
            }
        });
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
