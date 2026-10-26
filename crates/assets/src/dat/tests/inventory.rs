//! Real-file metadata inventory; original data stays outside Git.
use super::*;

/// Runs only where the ignored original trial data exists (skips otherwise).
#[test]
fn real_trial_file_matches_the_reviewed_ids() {
    let Ok(path) = std::env::var("AOE_TRIAL_DAT") else {
        eprintln!("AOE_TRIAL_DAT unset; real-file check skipped");
        return;
    };
    let bytes = std::fs::read(path).expect("AOE_TRIAL_DAT must name the trial file");
    let file = decode(&bytes).unwrap();
    assert_eq!(file.version, "VER 5.7");
    assert_eq!(file.report.stopped, None);
    assert_eq!(file.graphics.len(), 5024);
    assert_eq!(
        (file.report.units_parsed, file.report.unit_slots),
        (720, 750)
    );
    let slp = |id: i16| file.graphic(id.into()).unwrap().slp_id;
    let villager = file.unit(83).unwrap();
    assert_eq!(slp(villager.standing[0]), 1479);
    assert_eq!(slp(villager.walking.unwrap()[0]), 1484);
    // The catalog's cavalry 3004/3008 is Light Cavalry art; Scout Cavalry is SCOUT_*.
    let light = file.unit(546).unwrap();
    assert_eq!(slp(light.standing[0]), 3004);
    assert_eq!(slp(light.walking.unwrap()[0]), 3008);
    assert_eq!(slp(file.unit(448).unwrap().standing[0]), 2085);
    let table = table::build(&file, None);
    let status = |label: &str| {
        table
            .checks
            .iter()
            .find(|c| c.label == label)
            .unwrap()
            .status
            .clone()
    };
    assert_eq!(status("Town Center (Dark Age main)"), "family");
    assert_eq!(status("Villager idle"), "match");
    assert_eq!(status("Villager walk"), "match");
    assert_eq!(status("Stone mining"), "match");
    assert_eq!(status("Gold mining"), "match");
    assert_eq!(status("Gold carrying"), "match");
    assert_eq!(status("Stone carrying"), "match");
    assert_eq!(status("Wood chopping"), "match");
    assert_eq!(status("Wood carrying"), "match");
    // 1490/1493 were the builder's death and stand rows (VMBLD_DN/FN).
    assert_eq!(status("Berry foraging"), "match");
    assert_eq!(status("Food carrying"), "match");
    assert_eq!(status("Farm tilling"), "match");
    let lumberjack = file.unit(123).unwrap();
    assert_eq!(
        lumberjack.tasks[0].graphics.map(|g| g > 0),
        [false, true, true, true]
    );
    if let Ok(drs) = std::fs::read("../../local-assets/trial/Data/graphics.drs") {
        let pack = table::slp_ids(&drs).unwrap();
        let table = table::build(&file, Some(&pack));
        let pack_of = |label: &str| {
            table
                .rows
                .iter()
                .find(|r| r.label == label)
                .unwrap()
                .pack
                .clone()
        };
        assert_eq!(pack_of("Palisade Wall"), "complete");
        assert_eq!(pack_of("Farm"), "absent");
        assert_eq!(pack_of("Lumber Camp"), "partial");
    }
}
