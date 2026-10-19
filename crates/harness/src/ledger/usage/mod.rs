//! `make usage`: what is left of each provider, read where it can be read
//! without a new credential. A reading that cannot be taken says
//! `unknown: <reason>`; nothing here guesses.
mod claude;
mod codex;
mod openrouter;
mod time;

#[cfg(test)]
mod tests;

pub(super) use time::iso;

use std::path::Path;

pub(super) const WINDOW_DAYS: u64 = 7;

/// The whole remaining-usage section at `now` (seconds since the epoch).
pub(super) fn report(home: &Path, now: u64) -> String {
    let since = now.saturating_sub(WINDOW_DAYS * 86_400);
    format!(
        "Remaining usage (as of {})\n\n{}\n{}\n{}",
        iso(now),
        claude::section(&home.join(".claude/projects"), since),
        codex::section(&home.join(".codex/sessions"), now),
        openrouter::section(),
    )
}
