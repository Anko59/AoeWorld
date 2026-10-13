//! `make load-status` and `make ship-status`: who holds the heavy slots. Read-only.
use super::{Config, Holder, SLOTS_VAR, guard::dir, queue};
use crate::{ship::lock::utc, slots::queue::Result};
use std::path::Path;

pub(crate) fn print_status(root: &Path) -> Result<()> {
    let config = Config::load(root)?;
    for line in render(config.slots, &queue::alive::<Holder>(&dir(root)?)?) {
        println!("{line}");
    }
    Ok(())
}

/// The first `slots` live tickets are running, the rest queue in arrival order.
pub(crate) fn render(slots: usize, holders: &[Option<Holder>]) -> Vec<String> {
    let mut lines = vec![format!(
        "heavy slots: {} of {slots} in use ({SLOTS_VAR} to change)",
        holders.len().min(slots)
    )];
    for (index, holder) in holders.iter().enumerate() {
        let (label, number, verb) = if index < slots {
            ("running", index + 1, "started")
        } else {
            ("queued", index + 1 - slots, "queued")
        };
        lines.push(match holder {
            Some(h) => format!(
                "  {label} {number}. `{}` on {} in {} ({verb} {})",
                h.command,
                h.branch,
                h.worktree,
                utc(h.started)
            ),
            None => format!("  {label} {number}. (unreadable ticket)"),
        });
    }
    lines
}
