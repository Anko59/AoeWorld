use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Review {
    schema: u16,
    pub(super) subject: Value,
    notes: String,
}
impl Review {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        let value = crate::input_json::parse(bytes, MAX_BYTES)?;
        let review: Self =
            serde_json::from_value(value).map_err(|_| "review has invalid closed schema")?;
        if review.schema != 1
            || review.notes.is_empty()
            || review.notes.len() > 4096
            || !review
                .notes
                .chars()
                .all(|character| !character.is_control())
            || !review
                .notes
                .chars()
                .any(|character| !character.is_whitespace())
        {
            return Err("review requires schema1 and nonempty printable bounded notes".into());
        }
        Ok(review)
    }
}
