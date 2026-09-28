use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::fmt;

pub const MAX_RESOURCE_CHANGES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceAmount {
    pub id: u64,
    pub remaining: u16,
}

/// None starts a complete sparse snapshot; Some requires the exact prior
/// overlay revision. Subscription revisions bind messages to the active view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceState {
    pub subscription_revision: u64,
    pub from_revision: Option<u64>,
    pub revision: u64,
    #[serde(deserialize_with = "bounded_changes")]
    pub changes: Vec<ResourceAmount>,
}

impl ResourceState {
    pub fn valid(&self) -> bool {
        self.changes.len() <= MAX_RESOURCE_CHANGES
            && self.subscription_revision != 0
            && self
                .from_revision
                .is_none_or(|from| from < self.revision && !self.changes.is_empty())
            && (self.from_revision.is_some() || self.revision >= self.changes.len() as u64)
            && self
                .changes
                .iter()
                .all(|change| change.id & 1 == 0 && change.id >> 37 == 0)
            && self.changes.windows(2).all(|pair| pair[0].id < pair[1].id)
    }
}

fn bounded_changes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ResourceAmount>, D::Error> {
    struct Changes;
    impl<'de> Visitor<'de> for Changes {
        type Value = Vec<ResourceAmount>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most 65536 resource changes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > MAX_RESOURCE_CHANGES)
            {
                return Err(serde::de::Error::custom("too many resource changes"));
            }
            let mut changes = Vec::new();
            while let Some(change) = sequence.next_element()? {
                if changes.len() == MAX_RESOURCE_CHANGES {
                    return Err(serde::de::Error::custom("too many resource changes"));
                }
                changes.push(change);
            }
            Ok(changes)
        }
    }
    deserializer.deserialize_seq(Changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GAMEPLAY_MAX_MESSAGE, GameplayServerMessage, decode_gameplay_server, encode_gameplay_server,
    };
    #[test]
    fn resource_wire_fixture_and_full_limit_fit_the_frame_bound() {
        let message = GameplayServerMessage::ResourceState(ResourceState {
            subscription_revision: 1,
            from_revision: None,
            revision: 1,
            changes: vec![ResourceAmount {
                id: 0,
                remaining: 0,
            }],
        });
        assert_eq!(
            encode_gameplay_server(&message).unwrap(),
            [6, 1, 0, 1, 1, 0, 0]
        );
        assert_eq!(
            decode_gameplay_server(&[6, 1, 0, 1, 1, 0, 0]).unwrap(),
            message
        );
        let changes = (0..MAX_RESOURCE_CHANGES)
            .map(|index| ResourceAmount {
                id: (1_u64 << 36) + index as u64 * 2,
                remaining: u16::MAX,
            })
            .collect();
        let full = GameplayServerMessage::ResourceState(ResourceState {
            subscription_revision: u64::MAX,
            from_revision: None,
            revision: u64::MAX,
            changes,
        });
        let bytes = encode_gameplay_server(&full).unwrap();
        assert!(bytes.len() < GAMEPLAY_MAX_MESSAGE);
        assert_eq!(decode_gameplay_server(&bytes).unwrap(), full);
    }
    #[test]
    fn malformed_and_oversized_resource_frames_are_rejected_on_decode() {
        let empty = GameplayServerMessage::ResourceState(ResourceState {
            subscription_revision: 1,
            from_revision: Some(1),
            revision: 2,
            changes: Vec::new(),
        });
        assert!(encode_gameplay_server(&empty).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&empty).unwrap()).is_err());
        let invalid = GameplayServerMessage::ResourceState(ResourceState {
            subscription_revision: 1,
            from_revision: Some(3),
            revision: 2,
            changes: Vec::new(),
        });
        assert!(encode_gameplay_server(&invalid).is_err());
        assert!(decode_gameplay_server(&postcard::to_allocvec(&invalid).unwrap()).is_err());
        let oversized = GameplayServerMessage::ResourceState(ResourceState {
            subscription_revision: 1,
            from_revision: None,
            revision: u64::MAX,
            changes: vec![
                ResourceAmount {
                    id: 0,
                    remaining: 0
                };
                MAX_RESOURCE_CHANGES + 1
            ],
        });
        assert!(decode_gameplay_server(&postcard::to_allocvec(&oversized).unwrap()).is_err());
    }
}
