use crate::devserver::{ensure::lock, layout::Layout};
use std::{sync::mpsc, thread, time::Duration};

#[test]
fn a_second_session_waits_until_the_first_lets_go() {
    let directory = tempfile::tempdir().expect("directory");
    let layout = Layout::at(&directory.path().join(".git"));
    let first = lock(&layout).expect("first lock");
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn({
        let layout = Layout::at(&directory.path().join(".git"));
        move || {
            let second = lock(&layout).expect("second lock");
            sender.send(()).expect("send");
            drop(second);
        }
    });
    assert!(
        receiver.recv_timeout(Duration::from_millis(400)).is_err(),
        "the second session took the lock while the first held it"
    );
    drop(first);
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the second session gets the lock once the first lets go");
    handle.join().expect("thread");
}
