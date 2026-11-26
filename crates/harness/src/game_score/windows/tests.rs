use super::service::service;
use super::{Record, TICKS_PER_MINUTE, series, window};
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
