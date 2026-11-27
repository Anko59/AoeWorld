use super::*;
use aoe_core::Tick;

#[test]
fn victory_is_optional_and_empty_maps_never_start_a_countdown() {
    let (mut world, monk, relic, monastery) = super::lifecycle_tests::setup();
    let owner = PlayerId(0);
    world.issue_pickup_relic(owner, monk, relic).unwrap();
    super::lifecycle_tests::until(&mut world, |w| w.carried_relic(monk) == Some(relic));
    world.issue_deposit_relic(owner, monk, monastery).unwrap();
    super::lifecycle_tests::until(&mut world, |w| w.monastery_relic_count(monastery) == 1);
    assert_eq!(world.relic_victory(), None);
    world.enable_relic_victory();
    assert_eq!(
        world.relic_victory(),
        Some(RelicVictory::Holding {
            player: owner,
            deadline: Tick(world.tick().0 + 1_000 * u64::from(world.config().tick_hz)),
        }),
    );
    let mut empty = GameWorld::new(world.config()).unwrap();
    empty.enable_relic_victory();
    empty.advance();
    assert_eq!(empty.relic_victory(), None);
}

#[test]
fn ejected_relic_cancels_the_clock_and_every_redeposit_restarts_it() {
    let (mut world, monk, relic, monastery) = super::lifecycle_tests::setup();
    let owner = PlayerId(0);
    world.enable_relic_victory();
    let stash = |world: &mut GameWorld| {
        world.issue_pickup_relic(owner, monk, relic).unwrap();
        super::lifecycle_tests::until(world, |w| w.carried_relic(monk) == Some(relic));
        world.issue_deposit_relic(owner, monk, monastery).unwrap();
        super::lifecycle_tests::until(world, |w| w.monastery_relic_count(monastery) == 1);
        world.tick().0 + 1_000 * u64::from(world.config().tick_hz)
    };
    let first = stash(&mut world);
    world.issue_eject_relic(owner, monastery).unwrap();
    assert_eq!(world.relic_victory(), None);
    let second = stash(&mut world);
    assert!(second > first);
    world.tick = Tick(second - 2);
    world.advance();
    assert_eq!(world.relic_winner(), None);
    world.advance();
    assert_eq!(world.relic_winner(), Some(owner));
    world.advance();
    assert_eq!(world.tick(), Tick(second));
}

#[test]
fn save_and_load_preserves_the_victory_deadline_at_a_different_tick_rate() {
    let (mut world, monk, relic, monastery) = super::lifecycle_tests::setup();
    let owner = PlayerId(0);
    world.enable_relic_victory();
    world.issue_pickup_relic(owner, monk, relic).unwrap();
    super::lifecycle_tests::until(&mut world, |w| w.carried_relic(monk) == Some(relic));
    world.issue_deposit_relic(owner, monk, monastery).unwrap();
    super::lifecycle_tests::until(&mut world, |w| w.monastery_relic_count(monastery) == 1);
    let Some(RelicVictory::Holding { deadline, .. }) = world.relic_victory() else {
        panic!("all relics should start the victory countdown");
    };
    let canonical = world.snapshot([0; 32]).unwrap();
    let bytes = postcard::to_allocvec(&canonical).unwrap();
    let mut resumed = GameWorld::new(world.config()).unwrap();
    resumed.set_tick_hz(10);
    resumed
        .restore_snapshot(&postcard::from_bytes(&bytes).unwrap(), [0; 32], &|| false)
        .unwrap();
    assert_eq!(
        resumed.relic_victory(),
        Some(RelicVictory::Holding {
            player: owner,
            deadline: Tick(deadline.0 / 2),
        })
    );
}
