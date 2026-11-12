//! Movement bookkeeping must not enumerate the sleeping economy's jobs.
use super::*;

#[test]
fn movement_tables_do_not_revisit_all_jobs_in_the_late_game() {
    let mut w = GameWorld::late_game_workload().unwrap();
    for _ in 0..8 {
        w.advance();
    }
    let work = count(&w, "tables_refresh");
    assert!(work.calls > 0, "the workload must exercise movement tables");
    assert_eq!(
        work.visited, 0,
        "movement enumerated sleeping job memberships"
    );
}
