use super::*;
#[test]
fn duplicates_at_every_depth_invalid_utf8_trailing_and_exact_byte_bounds() {
    for bytes in [
        b"{\"x\":1,\"x\":2}".as_slice(),
        b"{\"x\":[{\"nested\":1,\"nested\":1}]}",
        b"{}{}",
        &[0xff],
        b"{\"x\":NaN}",
    ] {
        assert!(parse(bytes, 4096).is_err());
    }
    assert_eq!(
        parse(b"{\"x\":[true,null,-1,1.5]}", 4096).unwrap(),
        serde_json::json!({"x":[true,null,-1,1.5]})
    );
    assert!(parse(b"{}", 2).is_ok());
    assert!(parse(b"{}", 1).is_err());
}
#[test]
fn fixed_depth_collection_and_node_limits_reject_without_overrides() {
    let deep = format!("{}null{}", "[".repeat(34), "]".repeat(34));
    assert!(parse(deep.as_bytes(), 4096).is_err());
    let array = format!("[{}]", vec!["0"; 16385].join(","));
    assert!(parse(array.as_bytes(), 4 * 1024 * 1024).is_err());
    let object = format!(
        "{{{}}}",
        (0..16385)
            .map(|n| format!("\"k{n}\":0"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(parse(object.as_bytes(), 4 * 1024 * 1024).is_err());
    let row = format!("[{}]", vec!["0"; 16000].join(","));
    let many = format!("[{}]", vec![row; 13].join(","));
    assert!(parse(many.as_bytes(), 4 * 1024 * 1024).is_err());
}
