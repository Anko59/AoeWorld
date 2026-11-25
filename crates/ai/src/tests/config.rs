//! `Config` keeps every tunable and default of `ai/aoe_ai/config.py`, by the same names.
use crate::config::Config;

#[test]
fn pacing_and_budget_defaults_match_python() {
    let c = Config::default();
    assert_eq!(c.decision_hz, 2.0);
    assert!(!c.build_bridges);
    assert_eq!(c.bridge_retry_seconds, 30);
    assert_eq!(c.orders_per_decision, 40);
    assert_eq!(c.site_queries_per_decision, 4);
    assert_eq!(c.land_queries_per_decision, 16);
    assert_eq!(c.idle_land_query_reserve, 8);
    assert_eq!(c.late_economy_min_villagers, 128);
    assert_eq!(c.land_retry_seconds, 10);
    assert_eq!(c.land_cache_entries, 1024);
}

#[test]
fn economy_defaults_match_python() {
    let c = Config::default();
    assert_eq!(c.base_villagers, 60);
    assert_eq!(c.castle_villagers, 28);
    assert_eq!(c.castle_army_min, 12);
    assert_eq!(c.imperial_villagers, 40);
    assert_eq!(c.imperial_army_min, 24);
    assert_eq!(c.town_center_queue, 2);
    assert_eq!(c.house_margin, 4);
    assert_eq!(c.house_pending, 2);
    assert_eq!(c.dropoff_distance, 9);
    assert_eq!(c.max_gather_distance, 70);
    assert_eq!(c.wood_gather_distance, 256);
    assert_eq!(c.market_gold_reserve, 1400);
    assert_eq!(c.farm_target, 6);
    assert_eq!(c.farm_refusal_wait_seconds, 30);
    assert_eq!(c.world_limit_wait_seconds, 30);
}

#[test]
fn military_and_scouting_defaults_match_python() {
    let c = Config::default();
    assert_eq!(c.attack_army_size, 12);
    assert_eq!(c.retreat_fraction, 0.4);
    assert_eq!(c.hit_reach, 12);
    assert_eq!(c.attack_refusal_wait_seconds, 30);
    assert_eq!(c.march_hop_tiles, 48);
    assert_eq!(c.march_stall_seconds, 60);
    assert_eq!(c.army_train_orders, 8);
    assert_eq!(c.scout_max_radius, 120);
    assert!(!c.castle_first);
    assert!(c.fortifications);
    assert_eq!(c.fort_lane_distances, [12, 18, 24]);
    assert_eq!(c.general_mix, [35, 30, 20, 10, 5]);
    assert!(c.general_train && c.defer_to_steward && c.save_for_management);
}

#[test]
fn struct_update_overrides_one_tunable_and_keeps_the_rest() {
    let c = Config {
        orders_per_decision: 3,
        ..Config::default()
    };
    assert_eq!(c.orders_per_decision, 3);
    assert_eq!(c.land_queries_per_decision, 16);
}
