use super::*;

#[test]
fn oracle_collision_accounting_is_nested_and_restored_after_a_panic() {
    let work = GroupWork::default();
    work.collision(3);
    GroupWork::unmeasured(|| {
        work.collision(500);
        GroupWork::unmeasured(|| work.collision(900));
    });
    work.collision(7);
    let mut metrics = RouteMetrics::default();
    work.snapshot(&mut metrics);
    assert_eq!(metrics.group_collision_work, 10);
    let panic = std::panic::catch_unwind(|| GroupWork::unmeasured(|| panic!("oracle")));
    assert!(panic.is_err());
    work.collision(1);
    work.snapshot(&mut metrics);
    assert_eq!(metrics.group_collision_work, 11);
}
