//! Inspect the exact WGSL supplied to WebGPU; scheduling is never a verdict.
use super::*;

#[wasm_bindgen_test]
fn ordinary_compilation_source_has_no_ground_mixing_path() {
    let ordinary = draw::shader_source(false);
    let mixing = draw::shader_source(true);
    for token in [
        "fn mixed(",
        "fn lattice(",
        "fn value(",
        "fn edge(",
        "fn junction_share(",
        "ground_mixing",
        "let mixing",
        "let junction",
        "var ground_count",
        "let bary",
        "cumulative",
    ] {
        assert!(
            !ordinary.contains(token),
            "ordinary compilation source still contains {token}"
        );
    }
    for helper in ["fn mixed(", "fn value(", "fn edge(", "fn junction_share("] {
        assert!(mixing.contains(helper), "mixing source lost {helper}");
    }
    assert_ne!(ordinary, mixing);
    for source in [&ordinary, &mixing] {
        assert!(!source.contains("// #"), "unresolved shader directive");
        let fragment = source.split("fn fs_main(").nth(1).unwrap();
        let sprite = fragment
            .split("if in.solid == 0u {")
            .nth(1)
            .unwrap()
            .split("if in.solid == 2u")
            .next()
            .unwrap();
        assert_eq!(sprite.matches("textureSampleLevel(").count(), 1);
        for terrain_read in ["textureLoad(", "directory(", "sheet(", "terrain_sample("] {
            assert!(!sprite.contains(terrain_read));
        }
        assert!(sprite.contains("if texel.a <= 0.0 { discard; }"));
        assert!(sprite.contains("return texel * in.color;"));
    }
}
