use aoe_protocol::ResumeToken;

fn token_hex(token: ResumeToken) -> String {
    token.0.iter().map(|byte| format!("{byte:02x}")).collect()
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
