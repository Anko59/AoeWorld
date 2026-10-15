//! Wait for machine load before starting heavy work; gate deadlines remain unchanged.
use super::queue::Result;
use crate::process::{
    Cancellation,
    load::{self, Load},
};
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(30 * 60);
const POLL: Duration = Duration::from_secs(30);
const REPORT: Duration = Duration::from_secs(5 * 60);

pub(super) fn wait(cancellation: &Cancellation) -> Result<()> {
    let start = Instant::now();
    wait_with(
        BOUND,
        cancellation,
        &mut load::read,
        &mut || start.elapsed(),
        &mut |duration| {
            let until = Instant::now() + duration;
            while !cancellation.cancelled() && Instant::now() < until {
                std::thread::sleep(
                    Duration::from_millis(100).min(until.saturating_duration_since(Instant::now())),
                );
            }
        },
        &mut |line| eprintln!("{line}"),
    )
}

pub(super) fn wait_with(
    bound: Duration,
    cancellation: &Cancellation,
    read: &mut dyn FnMut() -> Option<Load>,
    elapsed: &mut dyn FnMut() -> Duration,
    sleep: &mut dyn FnMut(Duration),
    say: &mut dyn FnMut(&str),
) -> Result<()> {
    let mut next_report = Duration::ZERO;
    let mut waited = false;
    loop {
        if cancellation.cancelled() {
            return Err("heavy load: cancelled before starting heavy work".into());
        }
        let now = elapsed();
        let busy = read().filter(Load::overloaded);
        let Some(load) = busy else {
            if waited {
                say(&format!(
                    "heavy load: free after {}s; starting",
                    now.as_secs()
                ));
            }
            return Ok(());
        };
        if now >= bound {
            return Err(format!(
                "heavy load: still {:.1} on {} cores after {} min; refusing to start heavy work",
                load.one,
                load.cores,
                bound.as_secs() / 60
            )
            .into());
        }
        if now >= next_report {
            say(&format!(
                "heavy load: waiting; 1-minute load {:.1} exceeds {} cores ({} min so far, bound {} min)",
                load.one,
                load.cores,
                now.as_secs() / 60,
                bound.as_secs() / 60
            ));
            next_report = now + REPORT;
        }
        waited = true;
        sleep(POLL.min(bound.saturating_sub(now)));
    }
}
