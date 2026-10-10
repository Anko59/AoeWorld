//! The branch a change is reviewed against: `dev`, or the parent branch of a
//! stacked pull request (`make ship SHIP_BASE=<branch>`). It only moves the
//! diff (always `refs/remotes/origin/<base>`, never a local name); review
//! policy is still read from origin/dev (`trusted`).
use std::cell::RefCell;

pub(crate) const DEV: &str = "dev";

thread_local! {
    static BASE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Restores the previous base when dropped, however the caller ends.
pub(crate) struct Scope(Option<String>);

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.0.take();
        BASE.with(|base| *base.borrow_mut() = previous);
    }
}

/// Review against `branch` until the returned guard drops.
pub(crate) fn scope(branch: &str) -> Scope {
    Scope(BASE.with(|base| base.borrow_mut().replace(branch.to_owned())))
}

/// The current base branch: `dev` unless a scope says otherwise.
pub(crate) fn current() -> String {
    BASE.with(|base| base.borrow().clone())
        .unwrap_or_else(|| DEV.to_owned())
}
