use super::*;

/// Row-major states (0 unexplored, 1 explored, 2 visible), plus sparse pixels
/// of the authoritative 256x256 strategic view. Missing pixels are black.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FogState {
    pub revision: u64,
    pub tick: Tick,
    pub region: TileRect,
    pub disabled: bool,
    #[serde(deserialize_with = "bounded_tiles")]
    pub tiles: Vec<u8>,
    #[serde(deserialize_with = "bounded_overview")]
    pub overview: Vec<(u16, u8)>,
}

impl FogState {
    pub fn valid(&self) -> bool {
        let Some(width) = self.region.max.x.checked_sub(self.region.min.x) else {
            return false;
        };
        let Some(height) = self.region.max.y.checked_sub(self.region.min.y) else {
            return false;
        };
        self.revision > 0
            && self.region.min.x >= 0
            && self.region.min.y >= 0
            && (1..=MAX_SUBSCRIPTION_TILES).contains(&width)
            && (1..=MAX_SUBSCRIPTION_TILES).contains(&height)
            && if self.disabled {
                self.tiles.is_empty() && self.overview.is_empty()
            } else {
                self.tiles.len() == (width * height) as usize
                    && self.tiles.iter().all(|s| *s <= 2)
                    && self.overview.len() <= 65_536
                    && self
                        .overview
                        .iter()
                        .all(|(_, state)| (1..=2).contains(state))
                    && self.overview.windows(2).all(|p| p[0].0 < p[1].0)
            }
    }
}

fn bounded<'de, D, T, const N: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Bounded<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "at most {N} fog entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(serde::de::Error::custom("fog collection exceeds limit"));
            }
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if values.len() == N {
                    return Err(serde::de::Error::custom("fog collection exceeds limit"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Bounded::<T, N>(PhantomData))
}
fn bounded_tiles<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    bounded::<D, u8, 262_144>(d)
}
fn bounded_overview<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(u16, u8)>, D::Error> {
    bounded::<D, (u16, u8), 65_536>(d)
}
