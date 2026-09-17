#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod playground;

#[cfg(target_arch = "wasm32")]
pub use playground::{GAME_ATLAS_SIDE, GameArt, GameFrame};

#[cfg(target_arch = "wasm32")]
mod game_renderer;
#[cfg(target_arch = "wasm32")]
pub use game_renderer::GameRenderer;
