//! Bounded JSON parsing that rejects duplicate keys before constructing a Value.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::{error::Error, fmt};

pub(crate) fn parse(bytes: &[u8], max_bytes: usize) -> Result<Value, Box<dyn Error>> {
    if bytes.len() > max_bytes {
        return Err("JSON byte limit exceeded".into());
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let mut budget = 200000usize;
    let value = Node {
        depth: 0,
        budget: &mut budget,
    }
    .deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}
struct Node<'a> {
    depth: usize,
    budget: &'a mut usize,
}
impl<'de> DeserializeSeed<'de> for Node<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        if self.depth > 32 || *self.budget == 0 {
            return Err(de::Error::custom("JSON nesting/node limit exceeded"));
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
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.into()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(Node {
            depth: self.depth + 1,
            budget: &mut *self.budget,
        })? {
            if values.len() == 16384 {
                return Err(de::Error::custom("JSON array limit exceeded"));
            }
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
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
#[cfg(test)]
mod tests;
