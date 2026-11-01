//! Host listener reservations shared by browser and WebDriver tests.
use std::net::TcpListener;

pub(crate) fn configured(count: u32) -> Result<Vec<TcpListener>, Box<dyn std::error::Error>> {
    let range = port_range(&std::env::var("AOE_E2E_PORT_RANGE").unwrap_or_default())?;
    Ok(reserve_ports(range, count)?)
}

pub(crate) fn port_range(text: &str) -> Result<Option<(u16, u16)>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let invalid =
        || "AOE_E2E_PORT_RANGE must be START-END with 1 <= START <= END <= 65535".to_owned();
    let (first, last) = text.split_once('-').ok_or_else(invalid)?;
    if [first, last]
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(invalid());
    }
    let first = first.parse::<u16>().map_err(|_| invalid())?;
    let last = last.parse::<u16>().map_err(|_| invalid())?;
    if first == 0 || first > last {
        return Err(invalid());
    }
    Ok(Some((first, last)))
}

pub(crate) fn reserve_port(range: Option<(u16, u16)>) -> std::io::Result<TcpListener> {
    let Some((first, last)) = range else {
        return TcpListener::bind("127.0.0.1:0");
    };
    for port in first..=last {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!("no free browser-test port in {first}-{last}"),
    ))
}

pub(crate) fn reserve_ports(
    range: Option<(u16, u16)>,
    count: u32,
) -> std::io::Result<Vec<TcpListener>> {
    (0..count).map(|_| reserve_port(range)).collect()
}
