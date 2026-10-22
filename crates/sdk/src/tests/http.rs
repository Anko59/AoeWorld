use crate::{Endpoint, parse_token};

#[test]
fn endpoints_parse_http_ws_and_bare_authorities() {
    for url in [
        "http://127.0.0.1:9000",
        "ws://127.0.0.1:9000/game/ws",
        "127.0.0.1:9000/",
    ] {
        let endpoint = Endpoint::parse(url).unwrap();
        assert_eq!(endpoint.authority, "127.0.0.1:9000");
        assert_eq!(endpoint.websocket_url(), "ws://127.0.0.1:9000/game/ws");
    }
    for bad in [
        "",
        "localhost",
        "https://host:1",
        "http://u@host:1",
        "http://:",
    ] {
        assert!(Endpoint::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn slot_tokens_are_exactly_48_hex_digits() {
    let token = parse_token(&"ab".repeat(24)).unwrap();
    assert_eq!(token.0, [0xab; 24]);
    for bad in ["", "ab", &"zz".repeat(24), &"ab".repeat(25)] {
        assert!(parse_token(bad).is_err());
    }
}
