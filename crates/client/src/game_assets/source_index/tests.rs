use super::{FrameIdentity, IndexedFrames};

struct Row(&'static str, u32);
impl FrameIdentity for Row {
    fn source(&self) -> &str {
        self.0
    }
    fn frame(&self) -> u32 {
        self.1
    }
}

#[test]
fn interleaved_sources_keep_original_indices_in_animation_order() {
    let rows = IndexedFrames::from(vec![
        Row("roof", 2),
        Row("actor", 1),
        Row("roof", 0),
        Row("actor", 0),
        Row("roof", 1),
        Row("roof", 99),
    ]);
    let sources = rows.sources();
    assert_eq!(sources.prefix("roof", 3), Some(vec![2, 4, 0]));
    assert_eq!(sources.prefix("actor", 2), Some(vec![3, 1]));
    assert_eq!(sources.len("roof"), 4);
    assert!(sources.contains("roof"));
    assert!(!sources.contains("absent"));
    assert_eq!(sources.prefix("absent", 0), Some(vec![]));
    assert_eq!(sources.prefix("absent", 1), None);
    assert_eq!(sources.prefix("roof", 4), None);
    assert_eq!(sources.prefix("roof", u32::MAX), None);
}

#[test]
fn reviewed_prefix_rejects_holes_and_duplicates_without_rejecting_unused_tail() {
    let rows = IndexedFrames::from(vec![Row("a", 0), Row("a", 1), Row("a", 9), Row("a", 9)]);
    assert_eq!(rows.sources().prefix("a", 2), Some(vec![0, 1]));
    assert_eq!(rows.sources().prefix("a", 3), None);
    let duplicate = IndexedFrames::from(vec![Row("a", 0), Row("a", 0), Row("a", 2)]);
    assert_eq!(duplicate.sources().prefix("a", 2), None);
}

#[test]
fn overlay_append_and_metadata_edits_invalidate_an_already_used_index() {
    let mut rows = IndexedFrames::from(vec![Row("base", 0)]);
    assert_eq!(rows.sources().prefix("base", 1), Some(vec![0]));
    rows.extend(IndexedFrames::from(vec![
        Row("overlay", 1),
        Row("overlay", 0),
    ]));
    assert_eq!(rows.sources().prefix("overlay", 2), Some(vec![2, 1]));
    rows[2].1 = 1;
    assert_eq!(rows.sources().prefix("overlay", 2), None);
    rows[2].1 = 0;
    rows[1].0 = "base";
    assert_eq!(rows.sources().prefix("base", 2), Some(vec![0, 1]));
    assert_eq!(rows.sources().prefix("overlay", 1), Some(vec![2]));
}

#[test]
fn large_catalogue_prefix_is_bounded_and_keeps_native_record_identity() {
    let rows = IndexedFrames::from(
        (0..157854)
            .rev()
            .map(|n| Row("native", n))
            .collect::<Vec<_>>(),
    );
    let expected = (157854 - 64..157854).rev().collect::<Vec<_>>();
    for _ in 0..8 {
        assert_eq!(rows.sources().prefix("native", 64), Some(expected.clone()));
    }
}

#[test]
fn mutable_overlay_iteration_cannot_leave_cached_source_names_behind() {
    let mut rows = IndexedFrames::from(vec![Row("base", 0), Row("overlay", 0)]);
    assert!(rows.sources().contains("overlay"));
    for row in &mut rows {
        if row.0 == "overlay" {
            row.0 = "merged";
        }
    }
    assert!(!rows.sources().contains("overlay"));
    assert_eq!(rows.sources().prefix("merged", 1), Some(vec![1]));
}
