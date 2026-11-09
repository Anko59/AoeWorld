use super::build::Shape;
use super::{divergence, run};

#[test]
fn side_keeps_the_owner_density() {
    let shape = |seats, scale| Shape {
        seats,
        scale,
        side_tiles: 0,
        seed: 1,
    };
    assert_eq!(shape(100, 1.0).side(), 2_000);
    assert_eq!(shape(100, 4.0).side(), 4_000);
    assert_eq!(shape(1_000, 1.0).side(), 6_324);
    assert_eq!(shape(4, 0.01).side(), 256);
    let fixed = Shape {
        side_tiles: 900,
        ..shape(100, 1.0)
    };
    assert_eq!(fixed.side(), 900);
}

#[test]
fn divergence_names_the_first_differing_tick() {
    let a = vec![
        (0, "x".to_owned()),
        (10, "y".to_owned()),
        (20, "z".to_owned()),
    ];
    let mut b = a.clone();
    assert_eq!(divergence(&a, &b), None);
    b[2].1 = "w".to_owned();
    assert_eq!(
        divergence(&a, &b),
        Some((20, "z".to_owned(), "w".to_owned()))
    );
}

/// A small closed-loop match replays to the same hashes on 1 and 2 workers.
#[test]
fn small_match_is_deterministic_across_workers() {
    let plan = |threads| run::Plan {
        shape: Shape {
            seats: 4,
            scale: 0.2,
            side_tiles: 400,
            seed: 3,
        },
        threads,
        ticks: 60,
        hash_every: 20,
        sample_every: 0,
    };
    let one = run::run(&plan(1)).expect("one worker");
    let two = run::run(&plan(2)).expect("two workers");
    assert_eq!(one.hashes.len(), 4);
    assert_eq!(one.hashes, two.hashes);
    assert!(one.report["setup"]["units"].as_u64().unwrap_or(0) > 20);
}
