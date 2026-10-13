use super::annotate;
use std::path::Path;

fn failure(count: usize) -> String {
    (0..count)
        .map(|n| {
            format!(
                "invalid stored map package /w/local-assets/map-packages/p{n}.json: missing field `preparation_notes` at line 1 column 10651"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn annotated(message: &str) -> String {
    annotate(message.into(), Path::new("/w/local-assets/map-packages")).to_string()
}

#[test]
fn one_invalid_package_is_named_with_the_way_out() {
    let original = failure(1);
    let text = annotated(&original);
    assert!(text.starts_with(&original));
    assert!(text.contains("files: /w/local-assets/map-packages/p0.json\n"));
    assert!(text.contains(
        "stale local data from before a format change: regenerate the packages, or move \
         /w/local-assets/map-packages aside (for example `mv local-assets/map-packages \
         local-assets/map-packages.stale`); CI has none"
    ));
}

#[test]
fn five_invalid_packages_name_three_and_count_the_rest() {
    let original = failure(5);
    let text = annotated(&original);
    assert!(text.starts_with(&original));
    assert!(text.contains("p0.json, /w/local-assets/map-packages/p1.json, /w/local-assets/map-packages/p2.json +2 more\n"));
    assert!(!text[original.len()..].contains("p3.json"));
}

#[test]
fn other_failures_are_unchanged() {
    for message in [
        "test server exited early: exit status: 1",
        "invalid renderer evidence: x.json",
    ] {
        assert_eq!(annotated(message), message);
    }
}
