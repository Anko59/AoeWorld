//! Sprite minification pixels: quarter-size box filtering, native-size
//! identity, and confinement to the sprite's own atlas rectangle, on every
//! backend that has sprite pixel tests (Canvas 2D kernel and real WebGL2).
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[path = "fixture.rs"]
pub(crate) mod fixture;

#[path = "canvas.rs"]
mod canvas_pixels;

#[path = "webgl.rs"]
mod webgl_pixels;

#[path = "terrain.rs"]
pub(crate) mod terrain_pixels;
