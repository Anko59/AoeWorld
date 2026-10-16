use super::cache::key;

#[test]
fn cache_keys_separate_text_voice_model_and_field_boundaries() {
    let original = key("model", "Kore", "a narration");
    assert_eq!(original, key("model", "Kore", "a narration"));
    assert_ne!(original, key("model", "Puck", "a narration"));
    assert_ne!(original, key("another model", "Kore", "a narration"));
    assert_ne!(original, key("model", "Kore", "a narration "));
    assert_ne!(key("ab", "c", "d"), key("a", "bc", "d"));
}
