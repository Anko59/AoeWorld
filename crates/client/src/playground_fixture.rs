use super::{Client, map};
use aoe_map::CompactChunk;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

thread_local! {
    static ACTIVE: RefCell<Option<Weak<RefCell<Client>>>> = const { RefCell::new(None) };
}

pub(super) fn set_active(client: &Rc<RefCell<Client>>) {
    ACTIVE.with(|active| *active.borrow_mut() = Some(Rc::downgrade(client)));
}

/// Test-only immutable terrain binding used by deterministic browser fixtures.
/// It changes only client residency metadata; gameplay authority stays server-owned.
#[wasm_bindgen]
pub fn activate_surface_fixture(content_hash: &str, chunk_json: &str) -> Result<(), JsValue> {
    let mut bytes = [0_u8; 32];
    if content_hash.len() != 64 {
        return Err(JsValue::from_str("surface fixture hash must be 32 bytes"));
    }
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&content_hash[index * 2..index * 2 + 2], 16)
            .map_err(|_| JsValue::from_str("surface fixture hash is not hexadecimal"))?;
    }
    let compact: Vec<CompactChunk> = serde_json::from_str(chunk_json)
        .map_err(|error| JsValue::from_str(&format!("invalid surface fixture chunk: {error}")))?;
    let mut chunks = Vec::with_capacity(compact.len());
    for chunk in compact {
        chunks.push(
            chunk
                .decode()
                .map_err(|error| JsValue::from_str(&format!("surface fixture chunk: {error}")))?,
        );
    }
    ACTIVE.with(|active| {
        let client = active
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| JsValue::from_str("browser client is not initialized"))?;
        {
            let mut client = client.borrow_mut();
            client.surface_fixture = true;
            client.map_content_hash = Some(bytes);
            client.focus_map_hash = None;
            map::clear_terrain_cache(&mut client);
            for chunk in chunks {
                map::install_fixture_chunk(&mut client, &chunk);
                let coordinate = (chunk.x, chunk.y);
                client.terrain_chunks.insert(coordinate, chunk);
                client.terrain_discovered.insert(coordinate);
            }
        }
        Ok(())
    })
}
