use super::Shard;

#[test]
fn a_shard_is_i_of_n_within_bounds() {
    assert_eq!(Shard::parse("1/3"), Ok(Shard { index: 1, count: 3 }));
    assert_eq!(Shard::parse("3/3").map(|s| s.to_string()), Ok("3/3".into()));
    for invalid in [
        "", "0/3", "4/3", "1/0", "1/17", "1", "a/3", "1/b", "-1/3", "+1/3", "1/3/5", " 1/3",
    ] {
        assert!(Shard::parse(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn no_shard_means_the_whole_suite_and_coverage_refuses_a_shard() {
    assert_eq!(Shard::select("", None), Ok(None));
    assert_eq!(Shard::select("", Some("1")), Ok(None));
    assert_eq!(
        Shard::select("2/3", None),
        Ok(Some(Shard { index: 2, count: 3 }))
    );
    assert!(Shard::select("2/3", Some("1")).is_err());
    assert!(Shard::select("9/3", None).is_err());
}
