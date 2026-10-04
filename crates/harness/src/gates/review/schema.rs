use super::*;
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use std::fmt;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Review {
    schema: u16,
    pub(super) subject: Value,
    notes: String,
}
impl Review {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err("review exceeds4MiB".into());
        }
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        let mut budget = 200000usize;
        let value = Node {
            depth: 0,
            budget: &mut budget,
        }
        .deserialize(&mut decoder)?;
        decoder.end()?;
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
struct Node<'a> {
    depth: usize,
    budget: &'a mut usize,
}
impl<'de> DeserializeSeed<'de> for Node<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Value, D::Error> {
        if self.depth > 32 || *self.budget == 0 {
            return Err(de::Error::custom("review nesting/node limit exceeded"));
        }
        *self.budget -= 1;
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Node<'_> {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded duplicate-free JSON")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(value.into()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(Node {
            depth: self.depth + 1,
            budget: &mut *self.budget,
        })? {
            if values.len() == 16384 {
                return Err(de::Error::custom("review array limit exceeded"));
            }
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) || values.len() == 16384 {
                return Err(de::Error::custom(
                    "duplicate JSON key or object limit exceeded",
                ));
            }
            let value = map.next_value_seed(Node {
                depth: self.depth + 1,
                budget: &mut *self.budget,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
