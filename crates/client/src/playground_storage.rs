use aoe_protocol::ResumeToken;

pub(super) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[usize::from(byte >> 4)] as char);
        result.push(DIGITS[usize::from(byte & 15)] as char);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn hex_encoding_matches_lowercase_format_for_every_byte_and_hash_shape() {
        let bytes = (0..=255).collect::<Vec<u8>>();
        let expected = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex(&bytes), expected);
        assert_eq!(hex(&[]), "");
        assert_eq!(hex(&[0; 32]), "0".repeat(64));
        let token = ResumeToken(std::array::from_fn(|index| index as u8 * 11));
        assert_eq!(parse_token(token_hex(token)), Some(token));
        assert_eq!(parse_token("z".repeat(48)), None);
        assert_eq!(parse_token("0".repeat(47)), None);
    }
}

fn token_hex(token: ResumeToken) -> String {
    hex(&token.0)
}

fn parse_token(value: String) -> Option<ResumeToken> {
    if value.len() != 48 {
        return None;
    }
    let mut result = [0_u8; 24];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(ResumeToken(result))
}

pub(super) fn stored_token() -> Option<ResumeToken> {
    let storage = web_sys::window()?.session_storage().ok()??;
    parse_token(storage.get_item("aoeworld.resume-token").ok()??)
}

pub(super) fn save_token(token: Option<ResumeToken>) {
    let Some(storage) = web_sys::window().and_then(|window| window.session_storage().ok()?) else {
        return;
    };
    match token {
        Some(token) => {
            let _ = storage.set_item("aoeworld.resume-token", &token_hex(token));
        }
        None => {
            let _ = storage.remove_item("aoeworld.resume-token");
        }
    }
}
