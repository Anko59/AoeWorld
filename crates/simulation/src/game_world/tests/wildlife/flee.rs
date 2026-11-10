use super::*;

#[test]
fn hunters_can_shoot_deer_before_it_runs_out_of_bow_range() {
    let mut w = world();
    let hunter = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let deer = w.spawn_animal(AnimalKind::Deer, at(104, 100)).unwrap();
    w.issue_hunt(PlayerId(0), &[hunter], deer).unwrap();
    run(&mut w, 80);
    assert_eq!(w.animal(deer).unwrap().hp, 0);
    assert_eq!(w.animal(deer).unwrap().position, at(104, 100));
}

#[test]
fn deer_runs_one_short_burst_then_stops_when_the_threat_stays_put() {
    let mut w = world();
    w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let deer = w.spawn_animal(AnimalKind::Deer, at(102, 100)).unwrap();
    run(&mut w, 80);
    let stopped = w.animal(deer).unwrap().position;
    assert!(stopped.x <= at(105, 100).x);
    run(&mut w, 80);
    assert_eq!(w.animal(deer).unwrap().position, stopped);
    assert_eq!(w.animal(deer).unwrap().task, AnimalTask::Idle);
}

#[test]
fn hunting_and_boar_luring_replay_at_one_and_four_workers() {
    let play = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let mut w = world();
                let worker = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
                let deer = w.spawn_animal(AnimalKind::Deer, at(103, 100)).unwrap();
                let boar = w.spawn_animal(AnimalKind::Boar, at(108, 100)).unwrap();
                w.issue_hunt(PlayerId(0), &[worker], deer).unwrap();
                (0..200)
                    .map(|tick| {
                        if tick == 60 {
                            w.issue_hunt(PlayerId(0), &[worker], boar).unwrap();
                        }
                        w.advance();
                        w.canonical_hash()
                    })
                    .collect::<Vec<_>>()
            })
    };
    assert_eq!(play(1), play(4));
}
