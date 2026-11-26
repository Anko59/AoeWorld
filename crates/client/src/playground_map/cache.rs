//! One owned decoded composed chunk per cache entry.
use super::*;
use aoe_core::TileCoord;
use aoe_map::{
    CompactChunk, CompactChunkError, LandscapeAppearance, LandscapeChunk, LandscapeDecoration,
    LandscapeResource, LandscapeTile,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{RequestInit, Response};

#[derive(Clone, Debug)]
pub(crate) struct CachedChunk(LandscapeChunk);

impl CachedChunk {
    pub(crate) fn decode(compact: &CompactChunk) -> Result<Self, CompactChunkError> {
        compact.decode().map(Self)
    }

    pub(crate) fn coordinate(&self) -> (i32, i32) {
        (self.0.x, self.0.y)
    }

    pub(crate) fn base_tiles(&self) -> impl Iterator<Item = &Tile> {
        self.0.tiles.iter().map(|tile| &tile.terrain)
    }

    pub(crate) fn scene_tiles(
        &self,
    ) -> impl Iterator<Item = (TileCoord, &Tile, LandscapeAppearance)> {
        self.0
            .tiles
            .iter()
            .map(|tile| (tile.tile, &tile.terrain, tile.appearance))
    }

    pub(crate) fn tile_at(&self, width: i32, height: i32, x: i32, y: i32) -> Option<&Tile> {
        if x < 0 || y < 0 || x >= width || y >= height {
            return None;
        }
        self.0
            .tiles
            .binary_search_by_key(&(y, x), |tile| (tile.tile.y, tile.tile.x))
            .ok()
            .and_then(|index| self.0.tiles.get(index))
            .map(|tile| &tile.terrain)
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = (&ResourceNode, u8)> {
        self.0
            .resources
            .iter()
            .map(|resource| (&resource.node, resource.visual_family as u8))
    }

    pub(crate) fn decorations(&self) -> &[LandscapeDecoration] {
        &self.0.decorations
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        let chunk = &self.0;
        let owned = chunk
            .tiles
            .capacity()
            .saturating_mul(size_of::<LandscapeTile>())
            .saturating_add(
                chunk
                    .resources
                    .capacity()
                    .saturating_mul(size_of::<LandscapeResource>()),
            )
            .saturating_add(
                chunk
                    .decorations
                    .capacity()
                    .saturating_mul(size_of::<LandscapeDecoration>()),
            );
        size_of::<Self>().saturating_add(owned)
    }
}

impl From<LandscapeChunk> for CachedChunk {
    fn from(chunk: LandscapeChunk) -> Self {
        Self(chunk)
    }
}

pub(super) async fn fetch_chunk(
    content_hash: &str,
    x: i32,
    y: i32,
    controller: Option<&AbortController>,
) -> Result<CachedChunk, JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let options = RequestInit::new();
    // Only a live human seat receives the server's camera priority. The
    // server validates this token; AI traffic uses the low lane.
    if let Some(token) = super::super::storage::stored_token() {
        let headers = web_sys::Headers::new()?;
        headers.set(
            "X-AoeWorld-Controller-Token",
            &super::super::storage::hex(&token.0),
        )?;
        options.set_headers(&headers);
    }
    if let Some(controller) = controller {
        options.set_signal(Some(&controller.signal()));
    }
    let response: Response = JsFuture::from(
        window.fetch_with_str_and_init(&format!("/maps/{content_hash}/chunks/{x}/{y}"), &options),
    )
    .await?
    .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    let body = JsFuture::from(response.text()?).await?;
    #[cfg(feature = "frame-bench")]
    let _decode = super::super::perf::Scope::new(44);
    let compact = crate::game_assets::decode_chunk(
        &body.as_string().ok_or("map chunk response was not text")?,
    )?;
    let chunk =
        CachedChunk::decode(&compact).map_err(|error| JsValue::from_str(&error.to_string()))?;
    if chunk.coordinate() != (x, y) {
        return Err(JsValue::from_str("map chunk response coordinates mismatch"));
    }
    Ok(chunk)
}

#[path = "cache/tests.rs"]
#[cfg(test)]
mod tests;
