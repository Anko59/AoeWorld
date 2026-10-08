//! The only way out of a browser take's container, which has no network: a
//! Unix socket in the run's private work directory whose every connection is
//! forwarded to the app's loopback host and port, and nowhere else.
//! record.mjs listens on the app's port inside the container and pipes each
//! connection here, so Chromium reaches the app at its usual URL while WebRTC,
//! DNS and every other address find no network at all. The bridge is bounded
//! in open and total connections and lives exactly as long as the recording.
use std::{
    collections::HashMap,
    io,
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// The socket's name in the work directory, which record.mjs dials.
pub(crate) const SOCKET: &str = "app.sock";
/// Connections open at once; Chromium keeps a handful per origin.
pub(crate) const MAX_OPEN: usize = 64;
/// Connections over the whole recording.
pub(crate) const MAX_TOTAL: usize = 4096;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

type Open = Arc<Mutex<HashMap<u64, (UnixStream, TcpStream)>>>;

/// A running forwarder; dropping it closes every connection and the socket.
pub(crate) struct Bridge {
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    socket: PathBuf,
}

impl Bridge {
    /// Listen on `work`/[`SOCKET`] and forward to `host:port`, which must
    /// resolve to loopback addresses only.
    pub(crate) fn start(work: &Path, host: &str, port: u16) -> io::Result<Self> {
        Self::start_with(work, host, port, MAX_OPEN, MAX_TOTAL)
    }

    pub(crate) fn start_with(
        work: &Path,
        host: &str,
        port: u16,
        max_open: usize,
        max_total: usize,
    ) -> io::Result<Self> {
        let targets: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()?
            .filter(|address| address.ip().is_loopback())
            .collect();
        if targets.is_empty() {
            return Err(io::Error::other(format!(
                "the app bridge forwards only to loopback, not {host}:{port}"
            )));
        }
        let socket = work.join(SOCKET);
        // A deep checkout overflows the 108-byte socket address; the work
        // directory's descriptor names the same place in a few bytes.
        let directory = std::fs::File::open(work)?;
        let short = format!("/proc/self/fd/{}/{SOCKET}", directory.as_raw_fd());
        let listener = UnixListener::bind(short)
            .map_err(|error| io::Error::other(format!("{}: {error}", socket.display())))?;
        drop(directory);
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let accept = thread::Builder::new()
            .name("showcase-bridge".into())
            .spawn(move || serve(&listener, &targets, &flag, max_open, max_total))?;
        Ok(Self {
            stop,
            accept: Some(accept),
            socket,
        })
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn serve(
    listener: &UnixListener,
    targets: &[SocketAddr],
    stop: &AtomicBool,
    max_open: usize,
    max_total: usize,
) {
    let open: Open = Arc::default();
    let mut threads: Vec<JoinHandle<()>> = vec![];
    let mut total = 0_usize;
    while !stop.load(Ordering::SeqCst) {
        let client = match listener.accept() {
            Ok((client, _)) => client,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(POLL);
                continue;
            }
            Err(_) => break,
        };
        threads.retain(|thread| !thread.is_finished());
        let busy = open.lock().map_or(usize::MAX, |open| open.len());
        if total >= max_total || busy >= max_open {
            continue;
        }
        total += 1;
        let Some(upstream) = dial(targets) else {
            continue;
        };
        let (Ok(()), Ok(client_copy), Ok(upstream_copy)) = (
            client.set_nonblocking(false),
            client.try_clone(),
            upstream.try_clone(),
        ) else {
            continue;
        };
        let id = total as u64;
        if let Ok(mut open) = open.lock() {
            open.insert(id, (client_copy, upstream_copy));
        }
        let registry = Arc::clone(&open);
        if let Ok(thread) = thread::Builder::new()
            .name("showcase-bridge-pipe".into())
            .spawn(move || {
                pipe(client, upstream);
                if let Ok(mut open) = registry.lock() {
                    open.remove(&id);
                }
            })
        {
            threads.push(thread);
        }
    }
    if let Ok(open) = open.lock() {
        for (client, upstream) in open.values() {
            let _ = client.shutdown(Shutdown::Both);
            let _ = upstream.shutdown(Shutdown::Both);
        }
    }
    for thread in threads {
        let _ = thread.join();
    }
}

fn dial(targets: &[SocketAddr]) -> Option<TcpStream> {
    targets
        .iter()
        .find_map(|target| TcpStream::connect_timeout(target, CONNECT_TIMEOUT).ok())
}

/// Copy both ways until each side has finished writing.
fn pipe(client: UnixStream, upstream: TcpStream) {
    let (Ok(reply_to), Ok(replies)) = (client.try_clone(), upstream.try_clone()) else {
        return;
    };
    let back = thread::spawn(move || {
        let _ = io::copy(&mut &replies, &mut &reply_to);
        let _ = reply_to.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut &client, &mut &upstream);
    let _ = upstream.shutdown(Shutdown::Write);
    let _ = back.join();
}
