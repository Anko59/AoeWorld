//! The rows a staged scenario adds to the terminal table: what happened at its choke and, for two
//! armies, in its fight. A scenario without a choke shows none of them.
use super::table::grouped;
use crate::game_score::windows::WindowReport;

pub(super) type Row = (&'static str, fn(&WindowReport) -> String);

fn pair(a: u64, b: u64) -> String {
    format!("{} / {}", grouped(a), grouped(b))
}

fn tick(at: Option<u64>) -> String {
    at.map_or("none".to_owned(), grouped)
}

fn percent(permille: u64) -> String {
    format!("{}%", (permille + 5) / 10)
}

/// The numbers at the choke.
pub(super) const CHOKE: [Row; 6] = [
    ("units sent / across the line", |w| {
        w.service
            .choke
            .map_or("-".to_owned(), |c| pair(c.units_sent, c.crossed))
    }),
    ("most across the line in one game minute", |w| {
        w.service
            .choke
            .map_or("-".to_owned(), |c| grouped(c.crossed_per_minute_peak))
    }),
    ("first / last unit across (tick)", |w| {
        w.service.choke.map_or("-".to_owned(), |c| {
            format!(
                "{} / {}",
                tick(c.first_crossing_tick),
                tick(c.last_crossing_tick)
            )
        })
    }),
    ("waiting on the near side at the end", |w| {
        w.service
            .choke
            .map_or("-".to_owned(), |c| grouped(c.waiting_near_side))
    }),
    ("longest stand still inside the neck (ticks)", |w| {
        w.service
            .choke
            .map_or("-".to_owned(), |c| grouped(c.standing_still_inside_ticks))
    }),
    ("within two tiles of their goal at the end", |w| {
        w.service.choke.map_or("-".to_owned(), |c| {
            format!(
                "{} of {}{}",
                grouped(c.arrived_at_goal),
                grouped(c.units_sent),
                if c.all_arrived { " (all)" } else { "" }
            )
        })
    }),
];

/// The numbers of a fight between two armies.
pub(super) const BATTLE: [Row; 4] = [
    ("first blow (tick)", |w| {
        w.service
            .battle
            .map_or("-".to_owned(), |b| tick(b.first_blow_tick))
    }),
    ("units that fought / only queued behind", |w| {
        w.service
            .battle
            .map_or("-".to_owned(), |b| pair(b.fought, b.queued))
    }),
    ("fight ended (tick)", |w| {
        w.service
            .battle
            .map_or("-".to_owned(), |b| tick(b.fight_ended_tick))
    }),
    ("alive at the end, seat 0 / seat 1", |w| {
        w.service.battle.map_or("-".to_owned(), |b| {
            format!(
                "{} / {}",
                percent(b.alive_seat_0_permille),
                percent(b.alive_seat_1_permille)
            )
        })
    }),
];

/// The rows to show for these windows, after the usual ones.
pub(super) fn rows(windows: &[WindowReport]) -> Vec<Row> {
    let mut rows = Vec::new();
    if windows.iter().any(|w| w.service.choke.is_some()) {
        rows.extend(CHOKE);
    }
    if windows.iter().any(|w| w.service.battle.is_some()) {
        rows.extend(BATTLE);
    }
    rows
}

/// Whether a usual row says nothing in a staged scenario (no village, no economy, no AI seat).
pub(super) fn is_empty_in_a_staged_scenario(name: &str) -> bool {
    [
        "units / buildings produced",
        "villagers standing idle",
        "Steward orders",
        "General orders",
        "Vassal / Rust AI orders",
        "resources delivered",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}
