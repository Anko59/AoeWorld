//! Thread-local wait injection keeps the real Git/SSH paths testable offline.
use std::{cell::RefCell, time::Duration};

thread_local! {
    static WAITS: RefCell<Option<Vec<Duration>>> = const { RefCell::new(None) };
}

pub(super) fn record_wait(delay: Duration) -> bool {
    WAITS.with(|slot| {
        if let Some(waits) = slot.borrow_mut().as_mut() {
            waits.push(delay);
            true
        } else {
            false
        }
    })
}

pub(crate) fn without_sleep<T>(run: impl FnOnce() -> T) -> (T, Vec<Duration>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            WAITS.with(|slot| *slot.borrow_mut() = None);
        }
    }
    WAITS.with(|slot| assert!(slot.borrow_mut().replace(Vec::new()).is_none()));
    let _reset = Reset;
    let result = run();
    let waits = WAITS.with(|slot| slot.borrow_mut().take().unwrap());
    (result, waits)
}
