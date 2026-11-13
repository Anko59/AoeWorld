//! Applied-command recording and headless replay. Files are local benchmark inputs.
use super::{GameplayService, PendingAction, commands};
use aoe_core::PlayerId;
use aoe_simulation::TickPhase;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
    sync::Arc,
};
mod timing;
pub use timing::Timing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Serialize, Deserialize)]
pub struct RecordedCommand {
    pub player: PlayerId,
    action: PendingAction,
}
#[derive(Serialize, Deserialize)]
pub struct RecordedTick {
    /// Tick BEFORE application; commands retain their actual application order.
    pub tick: u64,
    pub commands: Vec<RecordedCommand>,
    pub hash: [u8; 32],
}
#[derive(Serialize, Deserialize)]
enum Row {
    Header {
        ticks: usize,
        start_tick: u64,
        initial_hash: [u8; 32],
        map_hash: Option<[u8; 32]>,
    },
    Tick(RecordedTick),
    Complete,
}
pub struct Recording {
    pub initial_hash: [u8; 32],
    pub map_hash: Option<[u8; 32]>,
    pub ticks: Vec<RecordedTick>,
}
impl Recording {
    /// Reject interrupted writes, extra rows, missing ticks and tick gaps.
    pub fn read(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        if file.metadata()?.len() > 1 << 30 {
            return Err("recording exceeds 1 GiB".into());
        }
        let mut rows = BufReader::new(file).lines();
        let Row::Header {
            ticks,
            start_tick,
            initial_hash,
            map_hash,
        } = serde_json::from_str(&rows.next().ok_or("missing header")??)?
        else {
            return Err("missing header".into());
        };
        if ticks == 0 || ticks > 1_000_000 {
            return Err("invalid tick count".into());
        }
        let mut result = Self {
            initial_hash,
            map_hash,
            ticks: Vec::new(),
        };
        for index in 0..ticks {
            let Row::Tick(tick) =
                serde_json::from_str(&rows.next().ok_or("incomplete recording")??)?
            else {
                return Err("expected tick".into());
            };
            if tick.tick != start_tick + index as u64 {
                return Err("recording has a tick gap".into());
            }
            result.ticks.push(tick);
        }
        if !matches!(
            serde_json::from_str::<Row>(&rows.next().ok_or("incomplete recording")??)?,
            Row::Complete
        ) || rows.next().is_some()
        {
            return Err("missing completion or extra rows".into());
        }
        Ok(result)
    }
}

pub(crate) struct Recorder {
    file: BufWriter<File>,
    remaining: usize,
    waiting_for_ai: bool,
}
impl Recorder {
    fn row(&mut self, row: &Row) -> Result<()> {
        serde_json::to_writer(&mut self.file, row)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        Ok(())
    }
}
impl GameplayService {
    /// Hold the saved state until the ordinary skirmish fleet is connected.
    pub(crate) async fn start_saved_recording(&self, path: &Path, ticks: usize) -> Result<()> {
        self.start_recording(path, ticks).await?;
        self.recording
            .lock()
            .map_err(|_| "recording lock poisoned")?
            .as_mut()
            .ok_or("recording is not armed")?
            .waiting_for_ai = true;
        self.set_paused(true);
        Ok(())
    }
    pub(crate) fn recording_ai_ready(&self) {
        if let Ok(mut guard) = self.recording.lock()
            && let Some(recorder) = guard.as_mut()
            && std::mem::take(&mut recorder.waiting_for_ai)
        {
            self.set_paused(false);
        }
    }
    /// Arm before a restored service becomes visible to the tick clock or AIs.
    pub async fn start_recording(&self, path: &Path, ticks: usize) -> Result<()> {
        if ticks == 0 || ticks > 1_000_000 {
            return Err("invalid recording tick count".into());
        }
        let world = self.world.read().await;
        let mut guard = self
            .recording
            .lock()
            .map_err(|_| "recording lock poisoned")?;
        if guard.is_some() {
            return Err("recording already armed".into());
        }
        let mut recorder = Recorder {
            file: BufWriter::new(File::create_new(path)?),
            remaining: ticks,
            waiting_for_ai: false,
        };
        recorder.row(&Row::Header {
            ticks,
            start_tick: world.tick().0,
            initial_hash: world.canonical_hash(),
            map_hash: self.map_content_hash,
        })?;
        *guard = Some(recorder);
        Ok(())
    }
    pub(super) fn record_inputs(
        &self,
        pending: &[(u64, PlayerId, super::PendingCommand)],
    ) -> Option<Vec<RecordedCommand>> {
        self.recording
            .lock()
            .ok()?
            .as_ref()
            .filter(|r| r.remaining > 0)
            .map(|_| {
                pending
                    .iter()
                    .map(|(_, player, command)| RecordedCommand {
                        player: *player,
                        action: command.action.clone(),
                    })
                    .collect()
            })
    }
    pub(super) fn record_finished(
        &self,
        tick: u64,
        commands: Vec<RecordedCommand>,
        world: &aoe_simulation::GameWorld,
    ) -> Result<()> {
        let mut guard = self
            .recording
            .lock()
            .map_err(|_| "recording lock poisoned")?;
        let recorder = guard.as_mut().ok_or("recording is not armed")?;
        recorder.row(&Row::Tick(RecordedTick {
            tick,
            commands,
            hash: world.canonical_hash(),
        }))?;
        recorder.remaining -= 1;
        if recorder.remaining == 0 {
            recorder.row(&Row::Complete)?;
            recorder.file.get_ref().sync_all()?;
        }
        Ok(())
    }
    /// No sessions, AI execution, transport, sleeps or disk writes. The callback
    /// receives instruction counts and CPU time before canonical hashing and bookkeeping.
    pub async fn replay_step(
        &self,
        tick: &RecordedTick,
        measured: &mut dyn FnMut(Timing),
    ) -> Result<[u8; 32]> {
        self.replay_step_using(tick, measured, timing::Probe::open)
            .await
    }
    async fn replay_step_using(
        &self,
        tick: &RecordedTick,
        measured: &mut dyn FnMut(Timing),
        open: impl FnOnce() -> std::io::Result<timing::Probe>,
    ) -> Result<[u8; 32]> {
        let mut world = self.world.write().await;
        if world.tick().0 != tick.tick {
            return Err("replay tick differs from world".into());
        }
        let probe = Arc::new(open()?);
        world.set_planning_probe(probe.clone());
        if let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() {
            generator.set_navigation_probe(probe.clone());
        }
        let started = probe.sample()?;
        let mut kinds = BTreeMap::<String, (u64, u64)>::new();
        let mut results = BTreeMap::<String, u64>::new();
        let mut expensive = Vec::new();
        for command in &tick.commands {
            // Names are built outside the sampled span; one extra counter read
            // per command is the attribution's only measured overhead.
            let name = serde_json::to_value(&command.action)
                .ok()
                .and_then(|v| match v {
                    serde_json::Value::Object(o) => o.keys().next().cloned(),
                    serde_json::Value::String(s) => Some(s),
                    _ => None,
                })
                .unwrap_or_default();
            let before = probe.sample()?;
            let issuer = if self
                .slots
                .iter()
                .any(|s| s.player == command.player && s.human)
            {
                aoe_simulation::OrderIssuer::Player
            } else {
                aoe_simulation::OrderIssuer::PythonAi
            };
            let result = world.with_order_issuer(issuer, |w| {
                w.record_order_decisions(1);
                commands::apply(w, command.player, command.action.clone())
            });
            let spent = (probe.sample()? - before).instructions;
            let key = format!("{name}:{result:?}");
            let row = kinds.entry(key.clone()).or_default();
            row.0 += 1;
            row.1 += spent;
            if result != aoe_protocol::CommandResult::Accepted {
                let action = serde_json::to_string(&command.action).unwrap_or_default();
                expensive.push((tick.tick, command.player.0, key.clone(), action, spent));
            }
            *results.entry(key).or_default() += 1;
        }
        probe.record("orders", probe.sample()? - started);
        let mut last = probe.sample()?;
        world.advance_with_probe(
            &mut |_| true,
            &mut |phase: TickPhase| match probe.sample() {
                Ok(now) => {
                    probe.record(timing::phase_name(phase), now - last);
                    last = now;
                }
                Err(_) => probe.fail(),
            },
        );
        let total = probe.sample()? - started;
        probe.finish()?;
        let (phases_instructions, phases_cpu_ns) = probe.drain();
        measured(Timing {
            instructions: total.instructions,
            cpu_ns: total.cpu_ns,
            phases_instructions,
            phases_cpu_ns,
            navigation_calls: probe.navigation_calls(),
            commands: kinds,
            results,
            expensive,
        });
        let hash = world.canonical_hash();
        world.take_resource_changes();
        world.take_remembered_building_changes();
        Ok(hash)
    }
}

#[cfg(test)]
mod tests;
