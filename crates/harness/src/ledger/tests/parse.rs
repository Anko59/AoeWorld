use super::{CLOSING, FULL, ONCE, pr};
use crate::ledger::parse::*;

#[test]
fn a_full_review_badge_is_read_once_though_the_body_repeats_it() {
    let found = badges(FULL);
    assert_eq!(found.len(), 1);
    let b = &found[0];
    assert!(!b.closing);
    assert_eq!(
        (b.grade, b.tier.as_str(), b.reviewers, b.rounds, b.confirmed),
        (8, "high", 5, 2, 4)
    );
    assert_eq!(
        (b.model.as_str(), b.effort.as_str()),
        ("Claude Sonnet 5.5", "high")
    );
}

#[test]
fn a_closing_badge_and_its_approved_grade() {
    let found = badges(CLOSING);
    assert!(found[0].closing);
    assert_eq!((found[0].grade, found[0].confirmed), (9, 0));
    assert_eq!(approved_grade(CLOSING), Some(8));
    assert_eq!(approved_grade(FULL), None);
}

#[test]
fn a_single_reviewer_low_tier_badge() {
    let line = "🟢 **Review: 9/10** (low tier · 1 reviewer · 1 round · gpt-6.1-sol medium): 0 confirmed · 0 disputed · 0 refuted";
    let b = &badges(line)[0];
    assert_eq!(
        (b.tier.as_str(), b.reviewers, b.rounds, b.model.as_str()),
        ("low", 1, 1, "GPT-6.1 Sol")
    );
}

#[test]
fn text_that_is_not_a_badge_is_ignored() {
    assert!(badges("Review: 8/10 maybe\n**Review: nine/10** (x)").is_empty());
    assert!(badges("").is_empty());
}

#[test]
fn written_by_lists_every_model_in_order() {
    assert_eq!(written_by(FULL), ["Claude Haiku 5.5", "Claude Opus 5.5"]);
    assert_eq!(written_by(ONCE), ["GPT-6.1 Sol"]);
    assert_eq!(
        written_by("- Written by GLM 5.3 flash (pi) from a brief."),
        ["GLM 5.3 flash"]
    );
    assert_eq!(
        written_by("- Written by Claude Opus 5.5."),
        ["Claude Opus 5.5"]
    );
    assert!(written_by("nobody wrote this").is_empty());
}

#[test]
fn trailers_credit_models_and_skip_bots() {
    let message = "Body.\n\nCo-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>\n\
Co-authored-by: github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>\n\
Co-authored-by: Claude Opus 5.5 <noreply@anthropic.com>\nSigned-off-by: Someone <a@b.c>\n";
    assert_eq!(
        trailer_models(message),
        ["Claude Haiku 5.5", "Claude Opus 5.5"]
    );
    assert!(trailer_models("no trailers").is_empty());
}

#[test]
fn review_ids_become_the_names_people_write() {
    assert_eq!(canonical("claude-opus-5-5"), "Claude Opus 5.5");
    assert_eq!(canonical("gpt-6.1-sol"), "GPT-6.1 Sol");
    assert_eq!(canonical("mystery-model"), "mystery-model");
}

#[test]
fn split_and_reverted_pull_requests() {
    assert!(is_split("Split the map crate", ""));
    assert!(is_split(
        "Part two",
        "This was split from #123 to stay small."
    ));
    assert!(!is_split("Fix a bug", "Nothing splits here."));
    let log = "Revert \"Gates: a thing (#401)\"\nGates: another (#402)\nRevert \"no number\"\nRevert \"Tests: x (#9)\"\nRevert \"Map: y (#300)\" (#450)\n";
    assert_eq!(
        reverted_numbers(log).into_iter().collect::<Vec<_>>(),
        [9, 300, 401],
        "a squash-merged revert names the reverted PR, not its own number"
    );
}

#[test]
fn a_pull_request_record_combines_body_and_trailers() {
    let p = pr(
        5,
        FULL,
        &[
            "Co-Authored-By: Claude Haiku 5.5 <n@a.com>",
            "Co-Authored-By: GPT-6.1 Sol <n@o.com>",
        ],
    );
    assert_eq!(
        p.models,
        ["Claude Haiku 5.5", "Claude Opus 5.5", "GPT-6.1 Sol"]
    );
    let r = p.review.unwrap();
    // Two cross-examination rounds inside the one full review that passed: it was the first.
    assert_eq!(
        (r.final_grade, r.first_grade, r.rounds, r.confirmed),
        (8, Some(8), Some(2), Some(4))
    );
    let once = pr(6, ONCE, &[]).review.unwrap();
    assert_eq!(
        (once.first_grade, once.rounds, once.confirmed),
        (Some(9), Some(1), Some(1))
    );
    let closing = pr(7, CLOSING, &[]).review.unwrap();
    assert!(closing.closing);
    assert_eq!(
        (
            closing.first_grade,
            closing.final_grade,
            closing.rounds,
            closing.confirmed
        ),
        (Some(8), 9, None, None)
    );
}

#[test]
fn a_description_without_a_badge_has_no_review() {
    assert!(pr(8, "Just prose.", &[]).review.is_none());
}

#[test]
fn codex_trailers_credit_the_codex_model() {
    let p = pr(
        9,
        "No written-by line.",
        &[
            "Co-Authored-By: GPT 6.1 sol <noreply@openai.com>",
            "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>",
        ],
    );
    assert!(
        p.models.contains(&"GPT-6.1 Sol".to_owned()),
        "{:?}",
        p.models
    );
}

#[test]
fn a_closing_review_after_a_failed_one_has_no_first_grade() {
    let after_failure = "### How\n\n🟢 **Closing review: 9/10** (high tier · 5 reviewers · 1 round · claude-opus-5-5 high): 0 confirmed · 0 disputed · 0 refuted · closing review of the fixes since `6c09a777adcf` against 5 carried finding(s)\n\n- Written by Claude Haiku 5.5 from a brief.\n";
    let r = pr(10, after_failure, &[]).review.unwrap();
    assert!(r.closing);
    assert_eq!(
        r.first_grade, None,
        "the failed first review is not in the description"
    );
}
