//! Ten simulated minutes, ordinary movement and delivery, no accelerated rules.
use super::*;
mod river;

#[derive(Default, Debug)]
struct Samples {
    moves: u64,
    switches: u64,
    job_changes: u64,
    working: u64,
    idle: u64,
    delivered: [u64; 4],
}

fn idle(w: &GameWorld, id: EntityId) -> bool {
    let u = w.unit(id).unwrap();
    !u.moving
        && !u.planning
        && matches!(
            u.task,
            UnitTask::Idle | UnitTask::Carrying | UnitTask::Moving
        )
        && !w.economy.builders.contains_key(&id)
        && !w.economy.garrison_orders.contains_key(&id)
}

#[test]
fn ten_minute_early_economy_keeps_workers_productive() {
    let (mut w, _, workers, _) = wood_realm(20);
    reserves(&mut w, 900, 600, 300, 200);
    let hz = w.config().tick_hz;
    let ticks = hz * 60 * 10;
    let mut samples = Samples::default();
    let mut previous = vec![(false, None, None); workers.len()];
    let mut previous_jobs = vec![(None, None, None); workers.len()];
    for tick in 0..ticks {
        // Small periodic purchases keep early stocks scarce. Income below is
        // read at every tick from actual deposit buckets, never stock growth.
        if tick % (hz * 60) == 0 && tick > 0 {
            let stock = &mut w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile;
            stock.food = stock.food.saturating_sub(300);
            stock.wood = stock.wood.saturating_sub(200);
        }
        let before = w
            .economy
            .income
            .get(&PlayerId(0))
            .map_or([0; 4], |i| i.recent(w.tick.0 + 1, hz));
        w.advance();
        let after = w
            .economy
            .income
            .get(&PlayerId(0))
            .map_or([0; 4], |i| i.recent(w.tick.0, hz));
        // Both observations use the post-advance epoch, excluding expiring
        // buckets before the tick so expiry cannot hide a new delivery.
        for ((total, new), old) in samples.delivered.iter_mut().zip(after).zip(before) {
            *total += new.saturating_sub(old);
        }
        for (index, &id) in workers.iter().enumerate() {
            let u = w.unit(id).expect("fixture workers survive");
            let moving = u.moving || u.planning;
            let dest = w.movement_order(id).map(|o| o.destination);
            let kind = w.gathering_kind(id);
            let tile = w.gather_tile(id);
            let builder = w.economy.builders.get(&id).map(|task| task.building);
            let (old_kind, old_tile, old_builder) = previous_jobs[index];
            samples.job_changes +=
                u64::from(old_kind.is_some() && kind.is_some() && old_kind != kind)
                    + u64::from(old_kind.is_some() && kind == old_kind && tile != old_tile)
                    + u64::from(builder.is_some() && builder != old_builder);
            previous_jobs[index] = (kind, tile, builder);
            let (was_moving, old_dest, old_kind) = previous[index];
            samples.moves += u64::from(moving && (!was_moving || dest != old_dest));
            samples.switches += u64::from(kind.is_some() && old_kind.is_some() && kind != old_kind);
            samples.working += u64::from(matches!(
                u.task,
                UnitTask::Gathering | UnitTask::Farming | UnitTask::Fishing
            ));
            samples.idle += u64::from(idle(&w, id));
            previous[index] = (moving, dest, kind.or(old_kind));
        }
        let work = w.steward_work();
        assert!(work.orders <= 64 && work.resource_searches <= 8 && work.site_checks <= 32);
        assert!(work.unit_visits <= 1_024 && work.building_visits <= 2_048);
        assert!(work.resource_nodes <= 1_608 * work.resource_searches);
        assert!(work.resource_blocks <= 25 * work.resource_searches);
    }
    let final_idle = workers.iter().filter(|id| idle(&w, **id)).count();
    let worker_ticks = u64::from(ticks) * workers.len() as u64;
    let report = format!(
        "ticks={ticks} workers={} moves/min={:.2} resource-switches/min={:.2} job-changes={} gathering-share={:.4} mean-idle={:.2} final-idle={final_idle} resources/min={:?}\n{samples:?}\n",
        workers.len(),
        samples.moves as f64 / 10.0,
        samples.switches as f64 / 10.0,
        samples.job_changes,
        samples.working as f64 / worker_ticks as f64,
        samples.idle as f64 / f64::from(ticks),
        samples.delivered.map(|n| n as f64 / 10.0),
    );
    eprintln!("{report}");
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/steward-stability");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("long-run.txt"), &report).unwrap();
    assert!(
        samples.working > 0 && samples.delivered.iter().sum::<u64>() > 0,
        "{report}"
    );
}
