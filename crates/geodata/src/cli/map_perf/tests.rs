use super::median;

#[test]
fn median_reports_the_middle_sample_or_the_middle_pair_mean() {
    // Odd counts take the middle sample of the sorted list.
    assert_eq!(median(&[7]), 7);
    assert_eq!(median(&[1, 5, 900]), 5);
    assert_eq!(median(&[3, 4, 5, 9, 40]), 5);
    // Even counts average the two middle samples instead of reporting the
    // upper one; the integer mean truncates.
    assert_eq!(median(&[4, 6]), 5);
    assert_eq!(median(&[1, 2, 8, 9]), 5);
    assert_eq!(median(&[10, 20, 30, 41]), 25);
}
