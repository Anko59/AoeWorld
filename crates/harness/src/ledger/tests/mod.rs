mod fetch;
mod parse;
mod table;

use super::record::{Pr, RawCommit, RawPr};

/// A description as `make ship` renders it: full review, three rounds.
pub(super) const FULL: &str = "<!-- level: high -->\n# 🧑 For humans\n\n### ✅ How\n\n\
🟡 **Review: 8/10** (high tier · 5 reviewers · 2 rounds · claude-sonnet-5-5 high): 4 confirmed · 0 disputed · 0 refuted\n\n\
review: full review — no passing review of this branch\n\n---\n\n# 🤖 For AI\n\n- Limits: none.\n\
- Written by Claude Haiku 5.5 from a brief; stop-check exclusion by Claude Opus 5.5.\n\n\
## Adversarial review\n\n🟡 **Review: 8/10** (high tier · 5 reviewers · 2 rounds · claude-sonnet-5-5 high): 4 confirmed · 0 disputed · 0 refuted\n";

/// Passed the first time, a single round.
pub(super) const ONCE: &str = "### ✅ How\n\n🟢 **Review: 9/10** (medium tier · 3 reviewers · 1 round · gpt-6.1-sol medium): 1 confirmed · 0 disputed · 0 refuted\n\n- Written by GPT-6.1 Sol (Codex) from a brief.\n";

/// A closing review after an approved first review.
pub(super) const CLOSING: &str = "### ✅ How\n\n🟡 **Closing review: 9/10** (high tier · 5 reviewers · 1 round · claude-opus-5-5 high): 0 confirmed · 0 disputed · 0 refuted · closing review of 9 lines in 1 file(s) that differ from the approved patch of `7719ec5af59c`, approved 8/10\n\nreview: closing review of 9 lines in 1 file(s) that differ from the approved patch of `7719ec5af59c`, approved 8/10\n\n- Written by Claude Sonnet 5.5 from a brief.\n";

pub(super) fn raw(number: u64, title: &str, body: &str, trailers: &[&str]) -> RawPr {
    RawPr {
        number,
        title: title.to_owned(),
        body: body.to_owned(),
        merged_at: format!("2026-10-{:02}T10:00:00Z", number % 28 + 1),
        head_ref_name: format!("branch-{number}"),
        commits: trailers
            .iter()
            .map(|t| RawCommit {
                message_body: format!("Message.\n\n{t}\n"),
            })
            .collect(),
    }
}

pub(super) fn pr(number: u64, body: &str, trailers: &[&str]) -> Pr {
    Pr::from_raw(&raw(number, "Title", body, trailers))
}
