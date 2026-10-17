use super::{base_urls, select};

#[test]
fn the_default_is_three_workers_and_the_count_is_bounded() {
    assert_eq!(select("", None, false), Ok(3));
    assert_eq!(select("1", None, false), Ok(1));
    assert_eq!(select("8", None, false), Ok(8));
    for invalid in ["0", "9", "-1", "+2", "two", " 2", "2 ", "1.5"] {
        assert!(select(invalid, None, false).is_err(), "{invalid:?}");
    }
}

#[test]
fn coverage_and_focused_runs_keep_one_server() {
    assert_eq!(select("", Some("1"), false), Ok(1));
    assert_eq!(select("1", Some("1"), false), Ok(1));
    assert!(select("3", Some("1"), false).is_err());
    assert_eq!(select("", None, true), Ok(1));
    assert_eq!(select("4", None, true), Ok(1));
}

#[test]
fn the_browser_gets_one_url_per_server_in_worker_order() {
    let addresses = [
        "127.0.0.1:4001".parse().unwrap(),
        "127.0.0.1:4002".parse().unwrap(),
    ];
    assert_eq!(
        base_urls(&addresses),
        "http://127.0.0.1:4001,http://127.0.0.1:4002"
    );
}
