use super::*;
use serde::Serializer;

mod delta;
pub use delta::{FogDelta, FogError};

/// Pixels of the 256x256 strategic view.
const MAX_OVERVIEW: usize = 65_536;
/// Tile bytes of the largest window: 512x512 tiles, four to a byte.
const MAX_PACKED: usize = 65_536;

/// The whole fog view of one subscription: row-major states (0 unexplored,
/// 1 explored, 2 visible), plus sparse pixels of the authoritative 256x256
/// strategic view. Missing pixels are black. On the wire the tiles travel
/// four to a byte, the first in the low bits. A client that holds the view is
/// sent its later changes as a `FogDelta`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FogState {
    pub revision: u64,
    pub tick: Tick,
    pub region: TileRect,
    pub disabled: bool,
    pub tiles: Vec<u8>,
    pub overview: Vec<(u16, u8)>,
}

impl FogState {
    pub fn valid(&self) -> bool {
        let Some(tiles) = tile_count(self.region) else {
            return false;
        };
        self.revision > 0
            && self.region.min.x >= 0
            && self.region.min.y >= 0
            && if self.disabled {
                self.tiles.is_empty() && self.overview.is_empty()
            } else {
                self.tiles.len() == tiles
                    && self.tiles.iter().all(|s| *s <= 2)
                    && self.overview.len() <= MAX_OVERVIEW
                    && self
                        .overview
                        .iter()
                        .all(|(_, state)| (1..=2).contains(state))
                    && self.overview.windows(2).all(|p| p[0].0 < p[1].0)
            }
    }
}

/// Tiles of a window of at most `MAX_SUBSCRIPTION_TILES` a side.
fn tile_count(region: TileRect) -> Option<usize> {
    let width = region.max.x.checked_sub(region.min.x)?;
    let height = region.max.y.checked_sub(region.min.y)?;
    let side = 1..=MAX_SUBSCRIPTION_TILES;
    (side.contains(&width) && side.contains(&height)).then(|| (width * height) as usize)
}

/// Up to four tile states in one byte, the first in the low bits. A state
/// that is none of the three travels as 3, which no decoder accepts.
fn pack(tiles: &[u8]) -> u8 {
    tiles
        .iter()
        .rev()
        .fold(0, |byte, state| (byte << 2) | (*state).min(3))
}

/// A packed byte that holds the state 3: there is no such state.
fn unknown_state(byte: u8) -> bool {
    byte & (byte >> 1) & 0x55 != 0
}

/// The tiles of a view of `count` tiles from its packed bytes. `None` when
/// the bytes are not such a view: another length, a state that does not
/// exist, or bits set past the last tile.
fn unpack(packed: &[u8], count: usize) -> Option<Vec<u8>> {
    let spare = count % 4;
    let past_the_end = spare != 0 && packed.last().is_some_and(|byte| byte >> (2 * spare) != 0);
    if packed.len() != count.div_ceil(4)
        || past_the_end
        || packed.iter().any(|byte| unknown_state(*byte))
    {
        return None;
    }
    let states = packed
        .iter()
        .flat_map(|byte| (0..4).map(move |slot| (byte >> (2 * slot)) & 3));
    let mut tiles = Vec::with_capacity(count);
    tiles.extend(states.take(count));
    Some(tiles)
}

/// Tile states that are sent four to a byte.
struct Packed<'a>(&'a [u8]);

impl Serialize for Packed<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.chunks(4).map(pack))
    }
}

/// A view as it is sent.
#[derive(Serialize)]
struct Sent<'a> {
    revision: u64,
    tick: Tick,
    region: TileRect,
    disabled: bool,
    tiles: Packed<'a>,
    overview: &'a [(u16, u8)],
}

/// A view as it arrives, its tiles still packed.
#[derive(Deserialize)]
struct Received {
    revision: u64,
    tick: Tick,
    region: TileRect,
    disabled: bool,
    #[serde(deserialize_with = "bounded_packed")]
    tiles: Vec<u8>,
    #[serde(deserialize_with = "bounded_overview")]
    overview: Vec<(u16, u8)>,
}

impl Serialize for FogState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let sent = Sent {
            revision: self.revision,
            tick: self.tick,
            region: self.region,
            disabled: self.disabled,
            tiles: Packed(&self.tiles),
            overview: &self.overview,
        };
        sent.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FogState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let received = Received::deserialize(deserializer)?;
        // A view without fog carries no tiles; any other fills its window.
        let count = if received.disabled {
            Some(0)
        } else {
            tile_count(received.region)
        };
        let tiles = count
            .and_then(|count| unpack(&received.tiles, count))
            .ok_or_else(|| serde::de::Error::custom("fog tiles do not fill their window"))?;
        Ok(Self {
            revision: received.revision,
            tick: received.tick,
            region: received.region,
            disabled: received.disabled,
            tiles,
            overview: received.overview,
        })
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
fn bounded_packed<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    bounded::<D, u8, MAX_PACKED>(d)
}
fn bounded_runs<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(u32, u32)>, D::Error> {
    bounded::<D, (u32, u32), MAX_PACKED>(d)
}
fn bounded_overview<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(u16, u8)>, D::Error> {
    bounded::<D, (u16, u8), MAX_OVERVIEW>(d)
}
