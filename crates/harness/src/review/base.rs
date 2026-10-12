//! The branch a change is reviewed against: `dev`, or the parent branch of a
//! stacked pull request (`make ship SHIP_BASE=<branch>`). It only moves the
//! diff (origin, an explicitly local parent, or a pinned commit); review
//! policy is still read from origin/dev (`trusted`).
use std::cell::RefCell;

pub(crate) const DEV: &str = "dev";

thread_local! {
    static BASE: RefCell<Option<Base>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct Base {
    branch: String,
    local: bool,
    commit: Option<String>,
}

/// Restores the previous base when dropped, however the caller ends.
pub(crate) struct Scope(Option<Base>);

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.0.take();
        BASE.with(|base| *base.borrow_mut() = previous);
    }
}

/// Review against `branch` until the returned guard drops.
pub(crate) fn scope(branch: &str) -> Scope {
    set(Base {
        branch: branch.into(),
        local: false,
        commit: None,
    })
}

/// An unpublished parent is named by its full local branch ref.
pub(crate) fn scope_local(branch: &str) -> Scope {
    set(Base {
        branch: branch.into(),
        local: true,
        commit: None,
    })
}

/// Freeze the selected base for gates, task, review, and its stored report.
pub(crate) fn scope_at(branch: &str, commit: &str) -> Scope {
    set(Base {
        branch: branch.into(),
        local: is_local(branch),
        commit: Some(commit.into()),
    })
}

fn set(value: Base) -> Scope {
    Scope(BASE.with(|base| base.borrow_mut().replace(value)))
}

pub(crate) fn is_local(branch: &str) -> bool {
    BASE.with(|base| {
        base.borrow()
            .as_ref()
            .is_some_and(|b| b.branch == branch && b.local)
    })
}

pub(crate) fn is_pinned(branch: &str) -> bool {
    BASE.with(|base| {
        base.borrow()
            .as_ref()
            .is_some_and(|b| b.branch == branch && b.commit.is_some())
    })
}

/// Never let tags or short names shadow the selected branch namespace.
pub(crate) fn reference(branch: &str) -> String {
    BASE.with(|base| {
        if let Some(base) = base.borrow().as_ref().filter(|b| b.branch == branch) {
            if let Some(commit) = &base.commit {
                return commit.clone();
            }
            if base.local {
                return format!("refs/heads/{branch}");
            }
        }
        format!("refs/remotes/origin/{branch}")
    })
}

/// The current base branch: `dev` unless a scope says otherwise.
pub(crate) fn current() -> String {
    BASE.with(|base| base.borrow().as_ref().map(|b| b.branch.clone()))
        .unwrap_or_else(|| DEV.to_owned())
}
