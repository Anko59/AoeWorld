use super::*;

#[test]
fn the_standing_issue_folds_its_oldest_sections_past_the_bound() {
    let mut body = String::new();
    for pr in 1..=MAX_SECTIONS as u64 + 1 {
        let finding = finding(
            &format!("f{pr}.rs"),
            Some(1),
            Severity::Minor,
            Status::Confirmed,
        );
        let section = plan(pr, PR, &report(vec![finding])).section.unwrap();
        body = standing_body(&body, &section);
    }
    assert_eq!(body.matches("## PR #").count(), MAX_SECTIONS, "{body}");
    assert!(
        body.contains("> Folded 1 earlier review section, up to #1."),
        "{body}"
    );
    assert!(!body.contains("## PR #1 "), "{body}");
    assert!(body.contains("## PR #2 "));
    // Folding again keeps one summary line, counting on.
    let section = plan(99, PR, &minor("z.rs", 1)).section.unwrap();
    let body = standing_body(&body, &section);
    assert_eq!(body.matches("> Folded").count(), 1);
    assert!(
        body.contains("> Folded 2 earlier review sections, up to #2."),
        "{body}"
    );
    assert_eq!(body.matches("## PR #").count(), MAX_SECTIONS);
}

#[test]
fn the_standing_body_folds_to_fit_the_limit_whatever_the_section_sizes() {
    let mut body = String::new();
    for pr in 1..=12u64 {
        let claims: Vec<_> = (0..40)
            .map(|n| {
                let mut item = finding(
                    &format!("p{pr}-{n}.rs"),
                    Some(n),
                    Severity::Minor,
                    Status::Confirmed,
                );
                item.reported.claim = "界".repeat(300);
                item
            })
            .collect();
        let section = plan(pr, PR, &report(claims)).section.unwrap();
        body = standing_body(&body, &section);
        let marked = marked_body(&body, STANDING_TITLE).expect("within the limit");
        assert!(marked.chars().count() <= BODY_LIMIT);
    }
    assert!(body.contains("> Folded "), "{body}");
    assert!(body.contains("## PR #12 "));
}
