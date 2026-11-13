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
