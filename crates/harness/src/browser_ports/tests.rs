use super::*;

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
