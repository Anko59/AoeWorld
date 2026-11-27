use super::arrival::{Orders, Point, Seen};
use super::income::Income;
use super::row::{Delay, Events, TickRow};
use std::collections::BTreeMap;

const TILE: i32 = 1024;

fn tiles(x: i32) -> Point {
    (x * TILE, 0)
}

fn unit(id: u32, x: Point, stepped: bool, goal: Option<i32>) -> Seen {
    Seen {
        id,
        position: x,
        stepped,
        goal: goal.map(tiles),
    }
}

/// An observer fed by hand, one tick at a time.
#[derive(Default)]
struct Watch {
    orders: Orders,
    events: Events,
    total: TickRow,
}

impl Watch {
    fn tick(&mut self, tick: u64, units: &[Seen]) {
        let mut row = TickRow::default();
        for seen in units {
            self.orders.watch(tick, *seen, &mut row, &mut self.events);
        }
        self.orders.finish(tick, &mut row, &mut self.events);
        self.total.orders_given += row.orders_given;
        self.total.orders_arrived += row.orders_arrived;
        self.total.orders_abandoned += row.orders_abandoned;
    }

    fn counts(&self) -> (u64, u64, u64) {
        (
            self.total.orders_given,
            self.total.orders_arrived,
            self.total.orders_abandoned,
        )
    }
}

fn delays(list: &[(u64, u64)]) -> Vec<Delay> {
    list.iter()
        .map(|&(tick, ticks)| Delay { tick, ticks })
        .collect()
}

#[test]
fn an_order_is_counted_once_and_arrives_within_two_tiles_of_its_goal() {
    let mut watch = Watch::default();
    watch.tick(1, &[unit(7, tiles(0), false, Some(10))]);
    watch.tick(2, &[unit(7, tiles(0), false, Some(10))]);
    watch.tick(3, &[unit(7, tiles(1), true, Some(10))]);
    watch.tick(4, &[unit(7, tiles(5), true, Some(10))]);
    assert_eq!(watch.counts(), (1, 0, 0), "not there yet");
    watch.tick(9, &[unit(7, tiles(8), true, Some(10))]);
    assert_eq!(watch.counts(), (1, 1, 0), "two tiles away is there");
    watch.tick(10, &[unit(7, tiles(10), false, None)]);
    assert_eq!(
        watch.counts(),
        (1, 1, 0),
        "an order that ended is not counted again"
    );
    assert_eq!(watch.events.first_steps, delays(&[(3, 2)]));
    assert_eq!(watch.events.arrivals, delays(&[(9, 8)]));
    assert_eq!(watch.orders.open(), 0);
}

#[test]
fn a_unit_that_stepped_in_the_tick_its_order_shows_has_a_delay_of_zero() {
    let mut watch = Watch::default();
    watch.tick(5, &[unit(1, tiles(0), true, Some(30))]);
    assert_eq!(watch.events.first_steps, delays(&[(5, 0)]));
    watch.tick(6, &[unit(1, tiles(1), true, Some(30))]);
    assert_eq!(
        watch.events.first_steps.len(),
        1,
        "one first step per order"
    );
}

#[test]
fn a_unit_already_at_its_goal_arrives_at_once() {
    let mut watch = Watch::default();
    watch.tick(5, &[unit(1, tiles(29), false, Some(30))]);
    assert_eq!(watch.counts(), (1, 1, 0));
    assert_eq!(watch.events.arrivals, delays(&[(5, 0)]));
}

#[test]
fn a_new_goal_gives_the_old_order_up_unless_it_had_arrived() {
    let mut watch = Watch::default();
    watch.tick(
        1,
        &[
            unit(1, tiles(0), true, Some(10)),
            unit(2, tiles(0), true, Some(10)),
        ],
    );
    watch.tick(
        4,
        &[
            unit(1, tiles(3), true, Some(20)),
            unit(2, tiles(9), true, Some(10)),
        ],
    );
    assert_eq!(
        watch.counts(),
        (3, 1, 1),
        "unit 1 changed its mind far from 10, unit 2 arrived"
    );
    watch.tick(
        6,
        &[
            unit(1, tiles(5), true, Some(20)),
            unit(2, tiles(10), true, Some(25)),
        ],
    );
    assert_eq!(
        watch.counts(),
        (4, 1, 1),
        "an order that had arrived is not given up when replaced"
    );
}

#[test]
fn an_order_that_ends_far_from_its_goal_is_given_up_and_one_that_ends_near_it_arrived() {
    let mut watch = Watch::default();
    watch.tick(
        1,
        &[
            unit(1, tiles(0), true, Some(10)),
            unit(2, tiles(0), true, Some(10)),
        ],
    );
    watch.tick(
        5,
        &[
            unit(1, tiles(4), false, None),
            unit(2, tiles(9), false, None),
        ],
    );
    assert_eq!(watch.counts(), (2, 1, 1));
    assert_eq!(watch.events.arrivals, delays(&[(5, 4)]));
}

#[test]
fn a_unit_that_leaves_the_world_ends_its_order_where_it_was_last_seen() {
    let mut watch = Watch::default();
    watch.tick(
        1,
        &[
            unit(1, tiles(0), true, Some(10)),
            unit(2, tiles(0), true, Some(10)),
        ],
    );
    watch.tick(
        3,
        &[
            unit(1, tiles(9), true, Some(10)),
            unit(2, tiles(2), true, Some(10)),
        ],
    );
    assert_eq!(watch.counts(), (2, 1, 0), "unit 1 arrived at tick 3");
    // Both are gone after tick 4 (killed, or inside a building).
    watch.tick(4, &[]);
    assert_eq!(
        watch.counts(),
        (2, 1, 1),
        "unit 2 was far from its goal when it left"
    );
    assert_eq!(watch.orders.open(), 0);
}

/// The engine's rule for `recent_deliveries`: credits are labelled with the
/// tick that made them; the window read at tick `t` holds the five-second
/// buckets `t / 50 - 11` through `t / 50`.
fn recent(credits: &BTreeMap<u64, [u64; 4]>, tick: u64) -> [u64; 4] {
    let epoch = tick / 50;
    let mut total = [0; 4];
    for (&label, amounts) in credits {
        let bucket = label / 50;
        if label <= tick && bucket <= epoch && epoch - bucket < 12 {
            for (sum, amount) in total.iter_mut().zip(amounts) {
                *sum += amount;
            }
        }
    }
    total
}

/// Credits that look like play: most ticks nothing, some ticks a load.
fn credits(ticks: u64) -> BTreeMap<u64, [u64; 4]> {
    let mut state = 12_345_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        state >> 33
    };
    let mut credits = BTreeMap::new();
    for tick in 1..=ticks {
        if next() % 3 == 0 {
            let kind = (next() % 4) as usize;
            let mut amounts = [0; 4];
            amounts[kind] = 10 + next() % 11;
            credits.insert(tick, amounts);
        }
    }
    credits
}

#[test]
fn every_ticks_credits_come_back_exactly_from_the_sliding_window() {
    let truth = credits(2_000);
    let mut income = Income::new(10);
    for tick in 1..=2_000 {
        let (found, fault) = income.credits(tick, recent(&truth, tick));
        assert_eq!(
            found,
            truth.get(&tick).copied().unwrap_or([0; 4]),
            "tick {tick}"
        );
        assert!(!fault, "tick {tick}");
    }
}

#[test]
fn a_window_read_every_six_hundred_ticks_is_not_a_clean_minute() {
    let every_tick: BTreeMap<u64, [u64; 4]> =
        (1..=1_300).map(|tick| (tick, [1, 0, 0, 0])).collect();
    assert_eq!(recent(&every_tick, 599)[0], 599, "ticks 1 to 599");
    assert_eq!(
        recent(&every_tick, 600)[0],
        551,
        "ticks 50 to 600: five seconds are lost"
    );
    assert_eq!(recent(&every_tick, 1_199)[0], 600);
}

#[test]
fn credits_summed_over_a_minute_equal_the_window_read_at_its_last_tick() {
    let truth = credits(1_800);
    let mut income = Income::new(10);
    let mut minute = [0_u64; 4];
    for tick in 1..=1_199 {
        let (found, _) = income.credits(tick, recent(&truth, tick));
        if tick >= 600 {
            for (sum, amount) in minute.iter_mut().zip(found) {
                *sum += amount;
            }
        }
    }
    assert_eq!(minute, recent(&truth, 1_199), "ticks 600 to 1,199");
}

#[test]
fn a_window_that_shrinks_without_a_bucket_leaving_is_reported_as_a_fault() {
    let mut income = Income::new(10);
    assert_eq!(income.credits(1, [30, 0, 0, 0]), ([30, 0, 0, 0], false));
    let (credits, fault) = income.credits(2, [10, 0, 0, 0]);
    assert!(fault, "30 became 10 inside one bucket");
    assert_eq!(credits, [0; 4], "a negative credit is not invented");
}
