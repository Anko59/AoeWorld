use super::*;

#[test]
fn join_slot_is_an_appended_variant_with_a_pinned_wire_form() {
    // Variants 0..=18 are the browser's; the slot join must stay at 19.
    let hello = ClientMessage::Hello {
        version: 3,
        resume_token: None,
    };
    assert_eq!(encode_client(&hello).unwrap(), [0, 3, 0]);
    let mut token = [0_u8; 24];
    for (index, byte) in token.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let join = ClientMessage::JoinSlot {
        version: VERSION,
        token: ResumeToken(token),
    };
    let mut expected = vec![19, 3];
    expected.extend(0..24_u8);
    assert_eq!(encode_client(&join).unwrap(), expected);
    assert_eq!(decode_client(&expected).unwrap(), join);
}
