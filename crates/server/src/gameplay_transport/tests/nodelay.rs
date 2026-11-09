//! The listener the server binary serves from. Without TCP_NODELAY the kernel
//! holds a small frame back until the peer acknowledges the one before it.
use axum::serve::Listener;
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn every_connection_the_server_listener_accepts_has_nodelay_set() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let _client = TcpStream::connect(address).await.unwrap();
    let (plain, _) = listener.accept().await.unwrap();
    assert!(
        !plain.nodelay().unwrap(),
        "a connection accepted from the bare listener has Nagle's algorithm on"
    );

    let mut listener = crate::nodelay(listener);
    assert_eq!(Listener::local_addr(&listener).unwrap(), address);
    for _ in 0..2 {
        let _client = TcpStream::connect(address).await.unwrap();
        let (accepted, _) = Listener::accept(&mut listener).await;
        assert!(
            accepted.nodelay().unwrap(),
            "an accepted connection must have TCP_NODELAY set"
        );
    }
}
