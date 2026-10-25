//! Exact build-assembled GPU modules for the native asset generator and tests.
pub fn browser_shaders() -> [&'static str; 2] {
    [
        include_str!(concat!(env!("OUT_DIR"), "/ordinary.wgsl")),
        include_str!(concat!(env!("OUT_DIR"), "/mixing.wgsl")),
    ]
}
