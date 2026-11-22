//! Parity test 1 (#978, 4.3), one-sided: lone movers off screen arrive no
//! later than on screen. Off screen they ride their routes (`ride.rs`).
use super::*;
use crate::UnitKind;

/// Twenty villagers of player 1 walk from the west bank of the river fixture
/// to the east bank through its dry gap (x 88, y 82 to 94), each on its own
/// order. Measures: the mean tick of arrival and the last, as shares of the
/// run, and (not compared) how many ticks a sampled walker did not move.
fn crossing(look: Look, jitter: &mut Jitter) -> Vec<f64> {
    const TICKS: u32 = 3_000;
    let mut world = build("river").world;
    let a = PlayerId(1);
    let walkers: Vec<_> = (0..20)
        .map(|n| {
            let at = jitter.at(60 + 2 * (n % 4), 112 + 3 * (n / 4));
            world.spawn_kind(a, UnitKind::Villager, at).unwrap()
        })
        .collect();
    look.seat(&mut world, &[a]);
    for (n, &id) in walkers.iter().enumerate() {
        world
            .issue_move(id, center(120 + 2 * (n as i32 % 4), 112))
            .unwrap();
    }
    let mut arrived = vec![TICKS; walkers.len()];
    let mut still = 0;
    for tick in 0..TICKS {
        let before = world.unit(walkers[0]).map(|u| u.position);
        world.advance();
        let unit = world.unit(walkers[0]);
        still += u32::from(unit.is_some_and(|u| u.moving && Some(u.position) == before));
        for (n, &id) in walkers.iter().enumerate() {
            if arrived[n] == TICKS && world.unit(id).is_some_and(|u| !u.moving && !u.planning) {
                arrived[n] = tick;
            }
        }
    }
    let n = walkers.len() as f64;
    let mean = arrived.iter().map(|&t| f64::from(t)).sum::<f64>() / n;
    let last = arrived.iter().copied().max().unwrap();
    vec![
        mean / f64::from(TICKS),
        f64::from(last) / f64::from(TICKS),
        f64::from(still),
    ]
}

#[test]
fn parity_1_lone_movers_off_screen_arrive_no_later() {
    let (watched, unwatched) = compare(
        "march",
        &["mean_arrival", "last_arrival", "still"],
        crossing,
    );
    assert!(
        watched.0[0].0 < 0.9,
        "the walkers did not arrive: {watched:?}"
    );
    assert_one_sided("march mean", (watched.0[0], unwatched.0[0]));
    assert_one_sided("march last", (watched.0[1], unwatched.0[1]));
    // Off screen a rider is moved every `RIDE_TICKS`, so it stands still most
    // ticks of its ride; on screen it steps every tick.
    let (on, off) = (watched.0[2].0, unwatched.0[2].0);
    assert!(
        off > on + 100.0,
        "a walker stood still {off} ticks off screen, {on} on"
    );
}
