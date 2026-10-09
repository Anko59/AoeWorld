//! Reuse Serde's loop-based fixed-array visitor without changing the wire form.
use serde::{Deserialize, Deserializer};

fn fixed<'de, D, const N: usize>(deserializer: D) -> Result<[u8; N], D::Error>
where
    D: Deserializer<'de>,
    [u8; N]: Deserialize<'de>,
{
    let mut bytes = [0; N];
    <[u8; N]>::deserialize_in_place(deserializer, &mut bytes)?;
    Ok(bytes)
}

pub(super) fn token<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 24], D::Error> {
    fixed(deserializer)
}
