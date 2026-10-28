use super::*;

/// Private original DAT evidence; portable CI skips it without the file.
#[test]
fn real_trial_shooters_release_on_the_fifth_attack_frame() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        eprintln!("no local trial data; real-file check skipped");
        return;
    };
    let file = decode(&bytes).unwrap();
    // Archer, Elite Skirmisher, Skirmisher and Crossbowman; the Mangonel
    // releases on its first frame.
    for (id, delay) in [(4, 5), (6, 5), (7, 5), (24, 5), (280, 0)] {
        assert_eq!(file.unit(id).unwrap().frame_delay, Some(delay), "unit {id}");
    }
}
