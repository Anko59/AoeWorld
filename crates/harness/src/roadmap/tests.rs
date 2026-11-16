use super::{parse_merged, problems, report, table};
use std::collections::BTreeSet;

const TEXT: &str = "# Area\n\n| item | asked | status | evidence | next step |\n| --- | --- | --- | --- | --- |\n| a | 1 | DONE | #10, #11 | |\n| b | 1 | DONE | no number | |\n| c | 1 | IN FLIGHT | branch | ship it |\n| d | 1 | DONE | #12 | |\n| e | 1 | maybe | | |\n\nprose | not a table\n\n| rule | since |\n| --- | --- |\n| ignored | 1 |\n";

#[test]
fn only_tables_with_a_status_column_are_rows() {
    let rows = table::parse("x", TEXT);
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[2].status, table::Status::InFlight);
    assert_eq!(rows[2].next, "ship it");
}

#[test]
fn a_done_row_needs_merged_pull_requests() {
    let rows = table::parse("x", TEXT);
    let merged: BTreeSet<u64> = [10, 11].into();
    let found = problems(&rows, &merged);
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(
        found
            .iter()
            .any(|p| p.contains("b") && p.contains("no pull request"))
    );
    assert!(found.iter().any(|p| p.contains("#12 is not a merged")));
    assert!(found.iter().any(|p| p.contains("status must start")));
}

#[test]
fn open_items_are_listed_in_row_order_with_counts() {
    let out = report(&table::parse("x", TEXT));
    assert!(out.contains("DONE        3"), "{out}");
    assert!(
        out.contains("  1. [IN FLIGHT] x / c\n       next: ship it"),
        "{out}"
    );
}

#[test]
fn merged_numbers_come_from_gh_json() {
    let merged = parse_merged("[{\"number\":5},{\"number\":9}]").unwrap();
    assert_eq!(merged, BTreeSet::from([5, 9]));
    assert!(parse_merged("not json").is_err());
}

#[test]
fn pull_request_numbers_are_deduplicated() {
    assert_eq!(table::pull_requests("#1, #2 and #1; issue 3"), vec![1, 2]);
}
