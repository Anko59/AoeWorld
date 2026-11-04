//! One fixed current census, with no unbounded allocation or older layout.
use super::UnitKind;
use serde::{
    Deserializer, Serializer,
    de::{Error, SeqAccess, Visitor},
    ser::SerializeTuple,
};
use std::fmt;
const KINDS: usize = UnitKind::ALL.len();

pub(super) fn serialize<S: Serializer>(
    counts: &[u32; KINDS],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut tuple = serializer.serialize_tuple(KINDS)?;
    for value in counts {
        tuple.serialize_element(value)?;
    }
    tuple.end()
}
pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<[u32; KINDS], D::Error> {
    struct Census;
    impl<'de> Visitor<'de> for Census {
        type Value = [u32; KINDS];
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("current unit census")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut counts = [0; KINDS];
            for value in &mut counts {
                *value = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("truncated unit census"))?;
            }
            Ok(counts)
        }
    }
    deserializer.deserialize_tuple(KINDS, Census)
}
