//! One entity that breaks a wire rule must not freeze the stream: every
//! Snapshot and Tick that carried it failed to encode, so a client could never
//! resync. The server repairs or omits the offender and says why, once.
use super::SendFailure;
use aoe_protocol::{
    GameplayServerMessage, GameplayViolation, encode_gameplay_server, gameplay_server_violations,
    sanitize_gameplay_server,
};
use std::{collections::BTreeMap, sync::Mutex};

/// How often each distinct (message, rule) failure has occurred.
pub(super) static SEEN: Mutex<BTreeMap<(&'static str, &'static str), u64>> =
    Mutex::new(BTreeMap::new());

/// Logs a violation the first time and then at every power of ten, so a rule
/// that breaks on every tick shows once with a growing count, not thousands
/// of lines.
fn report(violation: &GameplayViolation, how: &str) {
    let count = {
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        let count = seen.entry((violation.message, violation.rule)).or_insert(0);
        *count += 1;
        *count
    };
    if is_power_of_ten(count) {
        tracing::warn!(
            message = violation.message,
            rule = violation.rule,
            entity = ?violation.entity.map(|id| id.0),
            occurrences = count,
            how,
            detail = %violation.detail,
            "outgoing game state broke a wire rule"
        );
    }
}

pub(super) fn is_power_of_ten(mut n: u64) -> bool {
    while n >= 10 && n.is_multiple_of(10) {
        n /= 10;
    }
    n == 1
}

/// Encodes `message`; when it breaks a wire rule, repairs or omits the
/// offending entities, logs them, and encodes what remains.
pub(super) fn encode(mut message: GameplayServerMessage) -> Result<Vec<u8>, SendFailure> {
    if let Ok(bytes) = encode_gameplay_server(&message) {
        return Ok(bytes);
    }
    for violation in sanitize_gameplay_server(&mut message) {
        report(&violation, "entity repaired or omitted");
    }
    encode_gameplay_server(&message).map_err(|error| {
        let violations = gameplay_server_violations(&message);
        for violation in &violations {
            report(violation, "whole message rejected");
        }
        let rules: Vec<_> = violations.iter().map(|v| format!("{v}")).collect();
        SendFailure::Encode(format!("{error}: {}", rules.join("; ")))
    })
}
