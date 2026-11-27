use super::*;
use std::sync::mpsc;

#[test]
fn queued_connection_reserves_waiting_worker_before_it_wakes() {
    // The notified worker has yet to reacquire the queue lock. Its idle
    // registration is still live, but the first connection has claimed it.
    let mut todo = VecDeque::<Box<dyn FnMut() + Send>>::new();
    todo.push_back(Box::new(|| {}));
    let pool = TaskPool {
        sharing: Arc::new(Sharing {
            todo: Mutex::new(todo),
            condvar: Condvar::new(),
            active_tasks: AtomicUsize::new(MIN_THREADS),
            waiting_tasks: AtomicUsize::new(1),
        }),
    };
    let (release, released) = mpsc::channel::<()>();
    pool.spawn(Box::new(move || {
        // Keep the new worker out of the queue until the assertion completes.
        let _ = released.recv();
    }));
    assert_eq!(
        pool.sharing.todo.lock().unwrap().len(),
        1,
        "the second connection needs a worker of its own"
    );
    drop(release);
}
