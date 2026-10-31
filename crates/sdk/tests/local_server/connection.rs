//! A real TCP interruption for reconnect tests; no seat replacement is needed.
use std::net::SocketAddr;
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

pub(super) async fn proxy(target: SocketAddr) -> (SocketAddr, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (disconnect, lost) = oneshot::channel();
    tokio::spawn(async move {
        let (mut front, _) = listener.accept().await.unwrap();
        let mut back = TcpStream::connect(target).await.unwrap();
        tokio::spawn(async move {
            tokio::select! {
                _ = copy_bidirectional(&mut front, &mut back) => {},
                _ = lost => {},
            }
        });
        while let Ok((mut front, _)) = listener.accept().await {
            let mut back = TcpStream::connect(target).await.unwrap();
            tokio::spawn(async move {
                let _ = copy_bidirectional(&mut front, &mut back).await;
            });
        }
    });
    (address, disconnect)
}
