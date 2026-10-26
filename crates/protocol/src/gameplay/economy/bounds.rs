use super::*;

pub(crate) fn bounded<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Items<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Items<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {N} economy entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(serde::de::Error::custom("economy collection exceeds limit"));
            }
            let mut items = Vec::new();
            while items.len() < N {
                match sequence.next_element()? {
                    Some(item) => items.push(item),
                    None => return Ok(items),
                }
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("economy collection exceeds limit"));
            }
            Ok(items)
        }
    }
    deserializer.deserialize_seq(Items::<T, LIMIT>(std::marker::PhantomData))
}
pub(crate) fn bounded_players<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GameplayPlayerState>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_PLAYERS>(d)
}
pub(crate) fn bounded_buildings<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GameplayBuildingState>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_BUILDINGS>(d)
}
pub(crate) fn bounded_builders<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<EntityId>, D::Error> {
    bounded::<D, _, MAX_BUILD_GROUP>(d)
}
pub(crate) fn bounded_building_removals<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<EntityId>, D::Error> {
    bounded::<D, _, MAX_ECONOMY_BUILDINGS>(d)
}
pub(crate) fn bounded_training<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<TrainingState>, D::Error> {
    bounded::<D, _, MAX_TRAINING_QUEUE>(d)
}
