//! Real seat subscriptions and fan-out, measured after the headless window.
use super::{Census, Series, TICKS, hash, sample};
use crate::perf::ClientResult;
use aoe_core::{PlayerId, TileRect};
use aoe_protocol::GameplayServerMessage;
use aoe_server::GameplayService;
use aoe_simulation::{BuildingKind, GameWorld};
use serde::Serialize;
use std::{collections::BTreeMap, error::Error, time::Instant};
use tokio::sync::mpsc;

fn record(message: GameplayServerMessage, result: &mut ClientResult) -> Result<(), Box<dyn Error>> {
    result.bytes += aoe_protocol::encode_gameplay_server(&message)?.len() as u64;
    match message {
        GameplayServerMessage::Snapshot { units, .. } => {
            result.snapshots += 1;
            result.replicated += units.len() as u64;
            result.visible = result.visible.max(units.len());
        }
        GameplayServerMessage::Tick { .. } => result.deltas += 1,
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub(super) struct Evidence {
    before: Census,
    after: Census,
    initial_hash: String,
    final_hash: String,
    camera_updates: usize,
    activity: Vec<Census>,
}

pub(super) async fn measure(
    world: GameWorld,
    phases: &mut BTreeMap<String, Series>,
    wood: u32,
    initial_bound: usize,
) -> Result<(ClientResult, Evidence), Box<dyn Error>> {
    let before = Census::take(&world, wood, initial_bound);
    before.validate()?;
    let initial_hash = hash(&world);
    let seats: Vec<_> = (0..100)
        .map(|seat| {
            let player = PlayerId(seat);
            let primary = world
                .units()
                .find(|u| u.player == player)
                .ok_or("late-game seat has no units")?
                .id;
            Ok::<_, Box<dyn Error>>((player, primary, seat == 0))
        })
        .collect::<Result<_, _>>()?;
    let regions: Vec<_> = world
        .buildings()
        .filter(|b| b.kind == BuildingKind::TownCenter)
        .fold(BTreeMap::new(), |mut m, b| {
            m.entry(b.player).or_insert(TileRect::from_xywh(
                b.footprint.min.x - 40,
                b.footprint.min.y - 40,
                192,
                128,
            ));
            m
        })
        .into_values()
        .collect();
    let service = GameplayService::from_scale_world_slots(world, seats);
    let mut receivers = Vec::new();
    let mut cameras = Vec::new();
    for (info, region) in service.slot_infos().await.iter().zip(regions) {
        let (sender, receiver) = mpsc::channel(8192);
        let (id, _) = service
            .join_slot(info.token, sender)
            .await
            .map_err(|_| "late-game seat refused")?;
        service.subscribe(id, 1, region).await;
        receivers.push(receiver);
        cameras.push((id, region));
    }
    let mut result = ClientResult::default();
    let mut activity = Vec::new();
    let mut camera_updates = 0;
    for tick in 0..TICKS {
        for receiver in &mut receivers {
            while let Ok(message) = receiver.try_recv() {
                record(message, &mut result)?;
            }
        }
        let offered = Instant::now();
        for (seat, &(id, region)) in cameras.iter().enumerate() {
            // Four pans per seat, staggered over the window rather than a
            // synchronized hundred-camera burst. Subscribe uses real snapshots.
            if (tick + seat).is_multiple_of(32) {
                let offset = ((tick / 32 + 1) * 16) as i32;
                service
                    .subscribe(
                        id,
                        tick as u64 + 2,
                        TileRect::from_xywh(
                            region.min.x + offset,
                            region.min.y,
                            region.width(),
                            region.height(),
                        ),
                    )
                    .await;
                camera_updates += 1;
            }
        }
        sample(phases, "server_camera_orders", offered.elapsed().as_nanos());
        let started = Instant::now();
        service.tick().await;
        sample(phases, "server_tick", started.elapsed().as_nanos());
        sample(
            phases,
            "server_with_camera_tick",
            offered.elapsed().as_nanos(),
        );
        let census = service
            .inspect_scale_world(|world| Census::take(world, wood, initial_bound))
            .await;
        census.validate()?;
        activity.push(census);
        for (name, micros) in service.take_tick_phase_micros() {
            sample(
                phases,
                &format!("server_{name}"),
                u128::from(micros) * 1_000,
            );
        }
    }
    // Every seat must receive a real delta; failed delivery is never a timing pass.
    for receiver in &mut receivers {
        let mut tick = false;
        while let Ok(message) = receiver.try_recv() {
            tick |= matches!(message, GameplayServerMessage::Tick { .. });
            record(message, &mut result)?;
        }
        if !tick {
            return Err("late-game session missed its final delta".into());
        }
    }
    let (after, final_hash) = service
        .inspect_scale_world(|world| (Census::take(world, wood, initial_bound), hash(world)))
        .await;
    after.validate()?;
    if result.snapshots != 100 + camera_updates as u16 || camera_updates != 400 {
        return Err("late-game camera snapshot workload incomplete".into());
    }
    Ok((
        result,
        Evidence {
            before,
            after,
            initial_hash,
            final_hash,
            camera_updates,
            activity,
        },
    ))
}
