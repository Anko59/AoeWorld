use super::fixture_report;
use crate::perf_comment::reference::file_from;
use crate::perf_micro::dev::{DevFile, identity};

#[test]
fn the_dev_reference_file_records_the_newest_passing_run_or_the_reason_it_has_none() {
    let mut report = fixture_report();
    report
        .instructions
        .insert("simulation::late_game".to_owned(), Some(7));
    report
        .instructions
        .insert("simulation::advance".to_owned(), None);
    report.instruction_workloads = identity();
    match file_from(Ok(vec![(9, report)])) {
        DevFile::Measured(reference) => {
            assert_eq!(reference.run, 9);
            assert_eq!(
                reference.instructions.get("simulation::late_game"),
                Some(&7)
            );
            assert!(!reference.instructions.contains_key("simulation::advance"));
        }
        DevFile::Unavailable { unavailable } => panic!("unexpected: {unavailable}"),
    }
    for devs in [Ok(Vec::new()), Err("boom".to_owned())] {
        assert!(matches!(file_from(devs), DevFile::Unavailable { .. }));
    }
}
