//! Duplicate-aware bounded daemon packets; caller enforces the 32 KiB byte limit.
use serde_json::{Value, json};

pub(super) struct Packet(pub(super) Value);
impl<'de> serde::Deserialize<'de> for Packet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Packet;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded unique JSON packet")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Packet, E> {
                Ok(Packet(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Packet, E> {
                Ok(Packet(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Packet, E> {
                Ok(Packet(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Packet, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Packet(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Packet, E> {
                Ok(Packet(json!(v)))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Packet, E> {
                Ok(Packet(Value::String(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Packet, E> {
                Ok(Packet(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Packet, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<Packet>()? {
                    if values.len() >= 2048 {
                        return Err(serde::de::Error::custom("array bounds"));
                    }
                    values.push(value.0);
                }
                Ok(Packet(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Packet, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.len() >= 2048 || values.contains_key(&key) {
                        return Err(serde::de::Error::custom("object duplicate/bounds"));
                    }
                    values.insert(key, map.next_value::<Packet>()?.0);
                }
                Ok(Packet(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
