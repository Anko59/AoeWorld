use super::{base_urls, select};
use crate::browser_ports::{port_range, reserve_port, reserve_ports};

#[test]
fn a_fleet_reserves_distinct_ports_before_starting_any_server() {
    let listeners = reserve_ports(Some((8200, 8209)), 3).unwrap();
    let ports: std::collections::BTreeSet<_> = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap().port())
        .collect();
    assert_eq!(ports.len(), 3);
    assert!(ports.iter().all(|port| (8200..=8209).contains(port)));
}

#[test]
fn explicit_port_ranges_are_inclusive_and_validated() {
    assert_eq!(port_range(""), Ok(None));
    assert_eq!(port_range("8200-8209"), Ok(Some((8200, 8209))));
    assert_eq!(port_range("65535-65535"), Ok(Some((65535, 65535))));
    for invalid in [
        "0-8209",
        "8209-8200",
        "8200",
        "8200-",
        "8200-65536",
        " 8200-8209",
        "+8200-8209",
        "8200-8209-8210",
    ] {
        assert!(port_range(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn an_exhausted_port_range_never_falls_back_to_an_ephemeral_port() {
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    let error = reserve_port(Some((port, port))).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
}

#[test]
fn absent_port_range_keeps_os_allocation() {
    let listener = reserve_port(None).unwrap();
    assert_ne!(listener.local_addr().unwrap().port(), 0);
}

#[test]
fn occupied_ports_are_skipped_without_leaving_the_range() {
    for _ in 0..32 {
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let first = occupied.local_addr().unwrap().port();
        let Some(last) = first.checked_add(1) else {
            continue;
        };
        let Ok(neighbor) = std::net::TcpListener::bind(("127.0.0.1", last)) else {
            continue;
        };
        drop(neighbor);
        let listener = reserve_port(Some((first, last))).unwrap();
        assert_eq!(listener.local_addr().unwrap().port(), last);
        return;
    }
    panic!("could not reserve adjacent test ports");
}

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
