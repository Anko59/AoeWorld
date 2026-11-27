use super::service::service;
use super::{Record, TICKS_PER_MINUTE, series, window};
use crate::game_score::choke::ChokeTotals;
use crate::game_score::counters::{
    Delay, Events, ISSUERS, IssuerTotals, RouteTotals, Sample, TickRow,
};
use crate::game_score::scenarios::Span;

/// A run of `ticks` ticks: a row per tick (`fill` sets it) and a sample every
/// `every` ticks, the first at tick 0 and one at the end.
struct Run {
    rows: Vec<TickRow>,
    samples: Vec<Sample>,
    events: Events,
}

impl Run {
    fn new(ticks: u64, every: u64, fill: impl Fn(u64, &mut TickRow)) -> Self {
        let rows = (1..=ticks)
            .map(|tick| {
                let mut row = TickRow::default();
                fill(tick, &mut row);
                row
            })
            .collect();
        let mut samples: Vec<Sample> = (0..=ticks)
            .step_by(every as usize)
            .map(|tick| Sample {
                tick,
                ..Sample::default()
            })
            .collect();
        if samples.last().is_some_and(|last| last.tick != ticks) {
            samples.push(Sample {
                tick: ticks,
                ..Sample::default()
            });
        }
        Self {
            rows,
            samples,
            events: Events::default(),
        }
    }

    fn sample(&mut self, tick: u64, set: impl Fn(&mut Sample)) {
        let sample = self
            .samples
            .iter_mut()
            .find(|sample| sample.tick == tick)
            .expect("a sample at that tick");
        set(sample);
    }

    fn record(&self) -> Record<'_> {
        Record {
            rows: &self.rows,
            samples: &self.samples,
            events: &self.events,
        }
    }
}

fn span(id: &'static str, from: u64, to: u64) -> Span {
    Span { id, from, to }
}

#[test]
fn a_window_sums_the_ticks_after_its_start_through_its_end() {
    let run = Run::new(10, 1, |tick, row| {
        row.hits = tick;
        row.delivered = [tick, 0, 2 * tick, 0];
        row.steward.orders = 1;
        row.general.unit_visits = tick;
        row.units_born = u64::from(tick == 3 || tick == 6);
    });
    let found = service(run.record(), 2, 5).expect("a window");
    assert_eq!(found.fights.hits, 3 + 4 + 5);
    assert_eq!(found.economy.delivered.food, 12);
    assert_eq!(found.economy.delivered.gold, 24);
    assert_eq!(found.economy.delivered_total, 36);
    assert_eq!(found.management.steward.orders, 3);
    assert_eq!(found.management.general.unit_visits, 12);
    assert_eq!(
        found.size.units_produced, 1,
        "tick 3 is inside, tick 6 is not"
    );
}

#[test]
fn counters_the_engine_keeps_are_differences_of_the_samples_at_the_two_ends() {
    let mut run = Run::new(10, 5, |_, _| {});
    for (tick, searches, kills, born) in [(0, 0, 0, 0), (5, 10, 2, 4), (10, 40, 9, 10)] {
        run.sample(tick, |sample| {
            sample.routes = RouteTotals {
                orders: searches * 2,
                searches_started: searches,
                cache_hits: searches / 2,
                ..RouteTotals::default()
            };
            sample.kills = kills;
            sample.losses = kills + 1;
            sample.buildings_born = born;
        });
    }
    let found = service(run.record(), 5, 10).expect("a window");
    assert_eq!(found.planning.route_searches, 30);
    assert_eq!(found.planning.route_orders, 60);
    assert_eq!(found.planning.cache_hits, 15);
    assert_eq!((found.fights.kills, found.fights.losses), (7, 7));
    assert_eq!(found.size.buildings_produced, 6);
}

#[test]
fn orders_are_counted_by_who_gave_them_over_the_window() {
    let mut run = Run::new(10, 5, |_, _| {});
    let steward = ISSUERS
        .iter()
        .position(|name| *name == "steward")
        .expect("steward");
    run.sample(5, |sample| {
        sample.issuers[steward] = IssuerTotals {
            decisions: 3,
            orders: 10,
            accepted: 8,
            route_searches: 2,
        };
    });
    run.sample(10, |sample| {
        sample.issuers[steward] = IssuerTotals {
            decisions: 9,
            orders: 30,
            accepted: 25,
            route_searches: 7,
        };
    });
    let found = service(run.record(), 5, 10).expect("a window");
    let orders = &found.management.orders_by_issuer;
    assert_eq!(orders.len(), ISSUERS.len(), "every issuer has a line");
    assert_eq!(orders["steward"].orders, 20);
    assert_eq!(orders["steward"].route_searches, 5);
    assert_eq!(orders["vassal"], IssuerTotals::default());
}

#[test]
fn waiting_and_idle_numbers_are_read_at_the_end_and_over_the_samples_inside() {
    let mut run = Run::new(15, 5, |_, _| {});
    for (tick, waiting, p95, oldest, villagers, idle) in [
        (5, 50, 4, 9, 100, 10),
        (10, 80, 12, 30, 100, 30),
        (15, 20, 2, 5, 200, 60),
    ] {
        run.sample(tick, |sample| {
            sample.waiting = waiting;
            sample.wait_p95_ticks = p95;
            sample.oldest_wait_ticks = oldest;
            sample.villagers = villagers;
            sample.idle_villagers = idle;
            sample.units = 3 * villagers;
        });
    }
    let found = service(run.record(), 0, 15).expect("a window");
    assert_eq!(
        (found.planning.waiting_end, found.planning.waiting_peak),
        (20, 80)
    );
    assert_eq!(
        (
            found.planning.wait_p95_ticks_end,
            found.planning.wait_p95_ticks_peak
        ),
        (2, 12)
    );
    assert_eq!(found.planning.oldest_wait_ticks_peak, 30);
    assert_eq!(found.size.units, 600);
    // (10 + 30 + 60) idle over (100 + 100 + 200) villagers: 250 per mille.
    assert_eq!(found.size.idle_villagers_permille, 250);
    let later = service(run.record(), 10, 15).expect("a window");
    assert_eq!(
        later.size.idle_villagers_permille, 300,
        "the start sample is not inside"
    );
}

#[test]
fn delays_have_nearest_rank_percentiles_over_the_observations_inside_the_window() {
    let mut run = Run::new(100, 50, |_, _| {});
    run.events.arrivals = (1..=100)
        .map(|n| Delay { tick: n, ticks: n })
        .chain([Delay {
            tick: 101,
            ticks: 9_999,
        }])
        .collect();
    run.events.first_steps = vec![Delay { tick: 10, ticks: 4 }];
    let found = service(run.record(), 0, 100).expect("a window");
    let arrival = found.arrival.order_to_arrival_ticks;
    assert_eq!(
        (arrival.samples, arrival.p50, arrival.p95),
        (100, Some(50), Some(95))
    );
    let steps = found.arrival.order_to_first_step_ticks;
    assert_eq!((steps.samples, steps.p50, steps.p95), (1, Some(4), Some(4)));
    let early = service(run.record(), 0, 50).expect("a window");
    assert_eq!(early.arrival.order_to_arrival_ticks.samples, 50);
    let none = service(run.record(), 50, 100).expect("a window");
    assert_eq!(none.arrival.order_to_first_step_ticks, Default::default());
}

#[test]
fn the_share_that_arrived_is_of_the_orders_that_ended() {
    let run = Run::new(10, 5, |tick, row| {
        row.orders_given = 10;
        row.orders_arrived = if tick == 1 { 9 } else { 0 };
        row.orders_abandoned = if tick == 2 { 3 } else { 0 };
    });
    let found = service(run.record(), 0, 10).expect("a window");
    assert_eq!(found.arrival.orders_given, 100);
    assert_eq!(found.arrival.arrived_permille, Some(750));
    let nothing = service(run.record(), 5, 10).expect("a window");
    assert_eq!(nothing.arrival.arrived_permille, None, "no order ended");
}

#[test]
fn a_window_that_was_not_played_or_not_sampled_is_refused_by_name() {
    let run = Run::new(10, 5, |_, _| {});
    let error = service(run.record(), 5, 20).expect_err("past the end");
    assert!(error.contains("5-20"), "{error}");
    let error = service(run.record(), 3, 10).expect_err("no sample at tick 3");
    assert!(error.contains("tick 3"), "{error}");
}

#[test]
fn the_cost_of_a_window_is_wall_time_and_memory_and_stays_out_of_the_numbers() {
    let quick = Run::new(10, 5, |_, row| row.nanos = 1_000_000);
    let mut slow = Run::new(10, 5, |tick, row| row.nanos = 1_000_000 * tick);
    slow.sample(5, |sample| sample.resident_mib = 700);
    slow.sample(10, |sample| sample.resident_mib = 650);
    let (a, b) = (
        window(quick.record(), &span("w", 0, 10)).expect("window"),
        window(slow.record(), &span("w", 0, 10)).expect("window"),
    );
    assert_eq!(a.service, b.service, "the game is the same");
    assert_ne!(a.cost, b.cost);
    assert_eq!(a.cost.tick_seconds, 0.01);
    assert_eq!(b.cost.tick_seconds, 0.055);
    assert_eq!(b.cost.tick_ms.max, 10.0);
    assert_eq!(b.cost.tick_ms.p50, 6.0);
    assert_eq!(b.cost.peak_memory_mib, 700);
    assert_eq!(b.cost.instructions, None, "Callgrind comes later");
}

#[test]
fn cost_per_service_uses_wall_time_until_instructions_exist_and_says_which() {
    let run = Run::new(10, 5, |_, row| {
        row.nanos = 2_000_000;
        row.orders_arrived = 5;
        row.hits = 4;
        row.delivered = [100, 0, 0, 0];
    });
    let wall = window(run.record(), &span("w", 0, 10)).expect("window");
    assert_eq!(wall.efficiency.basis, "wall_time");
    assert_eq!(wall.efficiency.unit, "milliseconds");
    // 20 ms over 50 arrivals, 40 hits and 1,000 resources.
    assert_eq!(wall.efficiency.per_arrival, Some(0.4));
    assert_eq!(wall.efficiency.per_hit, Some(0.5));
    assert_eq!(wall.efficiency.per_thousand_resources_delivered, Some(20.0));
    assert_eq!(
        wall.efficiency.per_unit_produced, None,
        "no unit was produced"
    );
    let mut counted = wall.clone();
    counted.cost.instructions = Some(1_000_000);
    let efficiency = super::cost::efficiency(&counted.cost, &counted.service);
    assert_eq!(efficiency.basis, "instructions");
    assert_eq!(efficiency.per_arrival, Some(20_000.0));
}

#[test]
fn the_series_has_one_entry_per_game_minute_and_a_shorter_last_one() {
    let ticks = 2 * TICKS_PER_MINUTE + 250;
    let run = Run::new(ticks, 50, |_, row| row.delivered = [1, 1, 0, 0]);
    let minutes = series(run.record(), ticks).expect("a series");
    let edges: Vec<_> = minutes
        .iter()
        .map(|minute| (minute.minute, minute.from_tick, minute.to_tick))
        .collect();
    assert_eq!(edges, [(1, 0, 600), (2, 600, 1200), (3, 1200, 1450)]);
    let totals: Vec<_> = minutes
        .iter()
        .map(|minute| minute.service.economy.delivered_total)
        .collect();
    assert_eq!(totals, [1200, 1200, 500]);
    assert!(series(run.record(), 0).expect("no ticks").is_empty());
}

// The numbers at a choke (`Crossing`, `Battle`), from rows and the census the watch keeps.

fn watched(ticks: u64, totals: ChokeTotals, fill: impl Fn(u64, &mut TickRow)) -> Run {
    let mut run = Run::new(ticks, 10, fill);
    for sample in &mut run.samples {
        sample.choke = Some(totals);
    }
    run
}

#[test]
fn a_window_at_a_choke_counts_crossings_names_their_ticks_and_finds_the_best_game_minute() {
    let totals = ChokeTotals {
        sent: 40,
        sent_by_seat: [40, 0],
        near_side: 20,
        at_goal: 37,
        alive_by_seat: [40, 0],
        fought: 0,
    };
    let run = watched(1_300, totals, |tick, row| {
        row.crossed = match tick {
            200 => 3,
            250 => 10,
            900 => 7,
            _ => 0,
        };
        row.still_inside_peak = match tick {
            300 => 5,
            301 => 6,
            302 => 2,
            _ => 0,
        };
    });
    let found = service(run.record(), 0, 1_300).expect("a window");
    let crossing = found
        .choke
        .expect("a run watched at a choke has its numbers");
    assert_eq!(crossing.units_sent, 40);
    assert_eq!(crossing.crossed, 20);
    assert_eq!(
        crossing.crossed_per_minute_peak, 13,
        "ticks 200 and 250 are one game minute (600 ticks) apart at most; 900 is not with them"
    );
    assert_eq!(crossing.first_crossing_tick, Some(200));
    assert_eq!(crossing.last_crossing_tick, Some(900));
    assert_eq!(crossing.waiting_near_side, 20);
    assert_eq!(
        crossing.standing_still_inside_ticks, 6,
        "the longest, not the sum of the units that stood still"
    );
    assert_eq!(crossing.arrived_at_goal, 37);
    assert!(!crossing.all_arrived, "37 of 40");
    assert!(found.battle.is_none(), "one army is no battle");
}

#[test]
fn the_ticks_of_a_crossing_are_those_of_the_run_whatever_the_window() {
    let totals = ChokeTotals {
        sent: 10,
        sent_by_seat: [10, 0],
        alive_by_seat: [10, 0],
        at_goal: 10,
        ..ChokeTotals::default()
    };
    let run = watched(1_300, totals, |tick, row| {
        row.crossed = u64::from(tick == 200 || tick == 900);
    });
    let late = service(run.record(), 600, 1_300).expect("a window");
    let crossing = late.choke.expect("numbers");
    assert_eq!(
        (
            crossing.crossed,
            crossing.first_crossing_tick,
            crossing.last_crossing_tick
        ),
        (1, Some(900), Some(900))
    );
    assert!(crossing.all_arrived, "ten of ten");
    let early = service(run.record(), 0, 100).expect("a window");
    let nothing = early.choke.expect("numbers");
    assert_eq!(
        (
            nothing.crossed,
            nothing.crossed_per_minute_peak,
            nothing.first_crossing_tick,
            nothing.last_crossing_tick
        ),
        (0, 0, None, None)
    );
}

#[test]
fn two_armies_make_a_battle_with_a_first_blow_a_fight_and_two_shares_alive() {
    let totals = ChokeTotals {
        sent: 800,
        sent_by_seat: [400, 400],
        alive_by_seat: [100, 0],
        fought: 40,
        ..ChokeTotals::default()
    };
    let run = watched(1_000, totals, |tick, row| {
        row.hits = if tick >= 120 { 5 } else { 0 };
        row.fighters_new = match tick {
            120 => 30,
            130 => 10,
            _ => 0,
        };
        row.alive_by_seat = if tick < 700 { [300, 200] } else { [100, 0] };
    });
    let battle = service(run.record(), 0, 1_000)
        .expect("a window")
        .battle
        .expect("two armies make a battle");
    assert_eq!(battle.first_blow_tick, Some(120));
    assert_eq!(battle.fought, 40);
    assert_eq!(
        battle.queued, 760,
        "800 sent, 40 have struck or been struck"
    );
    assert_eq!(
        battle.fight_ended_tick,
        Some(700),
        "no unit of seat 1 is left alive"
    );
    assert_eq!(
        (battle.alive_seat_0_permille, battle.alive_seat_1_permille),
        (250, 0)
    );
    // Before the first blow, in a window of its own, nothing has happened.
    let before = service(run.record(), 0, 100)
        .expect("a window")
        .battle
        .expect("a battle");
    assert_eq!(
        (
            before.first_blow_tick,
            before.fought,
            before.fight_ended_tick
        ),
        (None, 0, None)
    );
}

#[test]
fn a_run_without_a_watch_has_no_choke_numbers_and_the_report_leaves_them_out() {
    let run = Run::new(10, 5, |_, _| {});
    let found = service(run.record(), 0, 10).expect("a window");
    assert!(found.choke.is_none() && found.battle.is_none());
    let json = serde_json::to_value(&found).expect("json");
    assert!(
        json.get("choke").is_none() && json.get("battle").is_none(),
        "{json}"
    );
}
