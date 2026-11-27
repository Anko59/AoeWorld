use super::{ALL, BATTLE, LATE, MARCH, NATURAL, Start, find, pull_request_set};
use std::collections::BTreeSet;

#[test]
fn every_scenario_has_a_name_and_windows_that_fit_and_are_sampled_at_their_edges() {
    let ids: BTreeSet<_> = ALL.iter().map(|scenario| scenario.id).collect();
    assert_eq!(ids.len(), ALL.len(), "two scenarios share an id");
    for scenario in ALL {
        assert!(!scenario.id.is_empty() && !scenario.what.is_empty());
        assert!(scenario.hash_every > 0 && scenario.sample_every > 0);
        let named: BTreeSet<_> = scenario.spans.iter().map(|span| span.id).collect();
        assert_eq!(
            named.len(),
            scenario.spans.len(),
            "{}: a window name twice",
            scenario.id
        );
        assert!(
            !named.contains("whole"),
            "{}: `whole` is added by the runner",
            scenario.id
        );
        for span in scenario.spans {
            assert!(span.from < span.to, "{}: {span:?} is empty", scenario.id);
            assert!(
                span.to <= scenario.ticks,
                "{}: {span:?} is past the end",
                scenario.id
            );
            assert_eq!(
                (
                    span.from % scenario.sample_every,
                    span.to % scenario.sample_every
                ),
                (0, 0),
                "{}: {span:?} starts or ends between two census samples",
                scenario.id
            );
        }
    }
}

#[test]
fn natural_is_the_owners_fresh_game_with_an_early_and_a_middle_window() {
    assert_eq!(NATURAL.start, Start::Fresh);
    assert_eq!(NATURAL.ticks, 10_000);
    let windows: Vec<_> = NATURAL
        .spans
        .iter()
        .map(|span| (span.id, span.from, span.to))
        .collect();
    assert_eq!(
        windows,
        [("early", 1_000, 3_000), ("middle", 8_000, 10_000)]
    );
}

#[test]
fn late_seeds_the_recipe_and_measures_after_a_warm_up() {
    assert!(matches!(LATE.start, Start::LateGame { percent } if percent > 0));
    let [window] = LATE.spans else {
        panic!("one late window");
    };
    assert_eq!(window.id, "late");
    assert!(window.from > 0, "the fresh AI brains need a warm-up first");
    assert_eq!(window.to, LATE.ticks, "nothing is played after the window");
}

#[test]
fn the_pull_request_set_is_what_make_game_score_plays_by_default() {
    let ids = |map: &str| -> Vec<_> {
        pull_request_set(map)
            .iter()
            .map(|scenario| scenario.id)
            .collect()
    };
    assert_eq!(
        ids(super::anatolia::MAP),
        ["natural", "late", "march", "battle"]
    );
    assert_eq!(
        ids("another map"),
        ["natural", "late"],
        "the staged scenarios are scenarios of one map each"
    );
}

#[test]
fn a_staged_scenario_is_played_on_its_map_and_refused_on_another_by_name() {
    for scenario in [&MARCH, &BATTLE] {
        let map = scenario.map().expect("a staged scenario has a map");
        assert_eq!(map.len(), 64);
        assert_eq!(scenario.applies_to(map), Ok(()));
        let error = scenario
            .applies_to(&"ab".repeat(32))
            .expect_err("another map");
        assert!(
            error.contains(scenario.id) && error.contains(&map[..8]) && error.contains("abababab"),
            "{error}"
        );
    }
    assert_eq!(NATURAL.map(), None);
    assert_eq!(LATE.applies_to("any map"), Ok(()));
}

#[test]
fn the_staged_scenarios_have_the_seats_of_their_armies_and_the_others_what_a_flag_says() {
    assert_eq!((MARCH.seats(100), BATTLE.seats(100)), (1, 2));
    assert_eq!((NATURAL.seats(100), LATE.seats(3)), (100, 3));
    assert!(MARCH.measures("choke.crossed") && !MARCH.measures("battle.queued"));
    assert!(BATTLE.measures("choke.crossed") && BATTLE.measures("battle.queued"));
    assert!(!NATURAL.measures("choke.crossed") && NATURAL.measures("planning.stuck"));
}

#[test]
fn a_scenario_is_found_by_id_and_a_wrong_id_names_the_right_ones() {
    assert_eq!(find("late").expect("late").id, "late");
    let error = find("later").expect_err("no such scenario");
    assert!(
        error.contains("later") && error.contains("natural") && error.contains("late"),
        "{error}"
    );
}

#[test]
fn a_shorter_run_keeps_the_windows_that_fit_and_always_reports_the_whole_run() {
    let ids = |ticks| -> Vec<_> { NATURAL.spans_for(ticks).iter().map(|s| s.id).collect() };
    assert_eq!(ids(10_000), ["early", "middle", "whole"]);
    assert_eq!(ids(5_000), ["early", "whole"]);
    assert_eq!(ids(50), ["whole"]);
    let whole = NATURAL.spans_for(5_000).pop().expect("a whole window");
    assert_eq!((whole.from, whole.to), (0, 5_000));
}

#[test]
fn the_census_is_taken_at_every_edge_of_every_window_and_every_game_minute() {
    for every in [50, 7, 1_000] {
        let at = NATURAL.sample_ticks(10_000, every);
        for needed in [0, 600, 1_200, 1_000, 3_000, 8_000, 10_000] {
            assert!(
                at.contains(&needed),
                "every {every}: no sample at tick {needed}"
            );
        }
        assert!(at.iter().all(|tick| *tick <= 10_000));
    }
    let short = LATE.sample_ticks(33, 10);
    assert_eq!(
        short.iter().copied().collect::<Vec<_>>(),
        [0, 10, 20, 30, 33]
    );
}

#[test]
fn a_flag_scales_the_late_recipe_and_never_a_fresh_start() {
    assert_eq!(NATURAL.late_percent(Some(30)), None);
    assert_eq!(NATURAL.late_percent(None), None);
    assert_eq!(LATE.late_percent(Some(30)), Some(30));
    let Start::LateGame { percent } = LATE.start else {
        panic!("late is a late game");
    };
    assert_eq!(LATE.late_percent(None), Some(percent));
}

#[test]
fn march_is_one_army_of_militia_and_battle_two_mixed_armies_of_the_same_size() {
    use super::super::staging::Side;
    use aoe_simulation::UnitKind;
    let Start::Staged(march) = MARCH.start else {
        panic!("march is staged");
    };
    let Start::Staged(battle) = BATTLE.start else {
        panic!("battle is staged");
    };
    let [army] = march.armies else {
        panic!("one army marches");
    };
    assert_eq!((army.seat, army.from, army.units()), (0, Side::Near, 800));
    assert!(army.mix.iter().all(|&(kind, _)| kind == UnitKind::Militia));
    assert!(!march.attack, "the march only moves");
    let [near, far] = battle.armies else {
        panic!("two armies meet");
    };
    assert_eq!((near.seat, near.from), (0, Side::Near));
    assert_eq!((far.seat, far.from), (1, Side::Far));
    assert_eq!(near.mix, far.mix, "the same army on each bank");
    assert!(near.mix.len() >= 3, "melee and ranged kinds");
    assert!(battle.attack, "the armies fight what they meet");
    assert_eq!(march.choke, battle.choke, "the same choke");
    assert!(
        march.choke.width_tiles < 10 && march.choke.length_tiles >= 3,
        "a neck an army of hundreds has to file through"
    );
    assert!(
        near.units() >= 300 && army.units() >= 300,
        "armies of several hundred"
    );
}
