mod load;
mod report;
mod scenario;
mod startup;
mod video;

use super::stats::*;

#[test]
fn censored_arrivals_stay_in_denominator_and_percentiles_use_nearest_rank() {
    let mut stats = Movement::new(vec![100, 100, 200, 100]);
    stats.arrive(0, 100);
    stats.arrive(1, 150);
    stats.arrive(2, 400);
    stats.arrive(0, 999); // first arrival only
    let result = stats.finish();
    assert_eq!(result.arrived, 3);
    assert_eq!(result.never_arrived, 1);
    assert_eq!(result.arrival_ppm, 750_000);
    assert_eq!(result.ticks_p50, Some(150));
    assert_eq!(result.ticks_p95, Some(400));
    assert_eq!(result.ratio_ppm_p50, Some(1_500_000));
    assert_eq!(result.ratio_ppm_p95, Some(2_000_000));
}

#[test]
fn obstruction_and_coherence_keep_unit_and_tick_denominators() {
    let mut stats = Movement::new(vec![10, 10]);
    stats.sample(0, true, 0, Some(2048));
    stats.sample(1, false, 1024, Some(0));
    stats.sample(0, true, 0, Some(4096));
    stats.sample(1, true, 2048, Some(0));
    let result = stats.finish();
    assert_eq!(result.obstructed_ticks, 3);
    assert_eq!(result.obstructed_ticks_worst_unit, 2);
    assert_eq!(result.obstructed_ticks_mean_milli, 1500);
    assert_eq!(result.units_moved_zero_tiles, 1);
    assert_eq!(result.slot_deviation_mean_subunits, Some(1536));
    assert_eq!(result.slot_deviation_worst_subunits, Some(4096));
    assert_eq!(result.out_of_shape_tick_ppm, Some(1_000_000));
}

#[test]
fn no_arrivals_or_no_shape_is_missing_not_a_perfect_score() {
    let result = Movement::new(vec![10]).finish();
    assert_eq!(result.arrival_ppm, 0);
    assert_eq!(result.never_arrived, 1);
    assert_eq!(result.ticks_p50, None);
    assert_eq!(result.slot_deviation_mean_subunits, None);
}

#[test]
fn damage_counts_all_impacts_and_uses_original_army_and_fight_duration() {
    let mut fight = Fight::new(2);
    fight.hit(0, 20, 7);
    fight.hit(0, 20, 5);
    fight.hit(1, 40, 8);
    fight.received(10);
    let report = fight.finish(1200, 20);
    assert_eq!(report.engaged_ppm, 1_000_000);
    assert_eq!(report.first_contact_ticks, Some(20));
    assert_eq!(report.damage_dealt_quarters, 20);
    assert_eq!(report.damage_per_unit_fight_minute_milli, 2500);
    assert_eq!(report.exchange_ratio_ppm, Some(2_000_000));
}

#[test]
fn zero_damage_received_has_no_finite_exchange_ratio() {
    let mut fight = Fight::new(1);
    fight.hit(0, 1, 4);
    assert_eq!(fight.finish(20, 20).exchange_ratio_ppm, None);
}

#[test]
fn stationary_contact_is_not_inferred_from_a_target_or_idle_tick() {
    let mut stats = Movement::new(vec![10]);
    stats.sample(0, false, 0, None);
    let result = stats.finish();
    assert_eq!(result.obstructed_ticks, 0);
    assert_eq!(result.units_moved_zero_tiles, 1);
}
