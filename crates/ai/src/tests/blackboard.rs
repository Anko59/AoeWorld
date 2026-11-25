//! The blackboard: counters, cooldowns and the defaults the managers rely on.
use crate::blackboard::Blackboard;

#[test]
fn a_new_blackboard_remembers_nothing_and_starts_its_clocks_in_the_past() {
    let board = Blackboard::default();
    assert!(board.tracked.is_empty() && board.bad_resources.is_empty());
    assert!(board.bad_sites.is_empty() && board.blocked_units.is_empty());
    assert!(board.plans.is_empty() && board.builders.is_empty());
    assert!(board.stats.is_empty() && board.cooldowns.is_empty());
    assert_eq!(board.home, None);
    assert_eq!(board.threat_until, -1);
    assert_eq!(board.raid_started, -1);
    assert_eq!(board.saving_tick, -1_000_000_000);
    assert_eq!(board.hold_villagers_tick, -1_000_000_000);
    assert!(!board.attacking);
    assert_eq!(board.attack_target, None);
}

#[test]
fn bump_counts_from_zero() {
    let mut board = Blackboard::default();
    board.bump("orders");
    board.bump("orders");
    board.bump("order:move");
    assert_eq!(board.stats.get("orders").copied(), Some(2));
    assert_eq!(board.stats.get("order:move").copied(), Some(1));
    assert_eq!(board.stats.get("other").copied(), None);
}

#[test]
fn a_cooldown_holds_until_its_last_tick() {
    let mut board = Blackboard::default();
    assert!(!board.cooling("farm:9", 100));
    board.cool("farm:9", 100, 50);
    assert!(board.cooling("farm:9", 100));
    assert!(board.cooling("farm:9", 149));
    assert!(!board.cooling("farm:9", 150));
    assert!(!board.cooling("farm:10", 100));
}

#[test]
fn a_full_cooldown_table_drops_expired_entries_so_it_stays_bounded() {
    let mut board = Blackboard::default();
    for i in 0..2048 {
        board.cooldowns.insert(format!("old:{i}"), 5);
    }
    board.cooldowns.insert("live".to_string(), 500);
    board.cool("new", 100, 10);
    assert_eq!(
        board.cooldowns.len(),
        2,
        "only the live and the new entry stay"
    );
    assert!(board.cooling("live", 100) && board.cooling("new", 100));
}
