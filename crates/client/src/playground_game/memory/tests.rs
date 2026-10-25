use super::Snapshot;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn snapshot_preserves_counts_and_renderer_pair_schema() {
    let snapshot = Snapshot {
        backend: "webgl2",
        counts: [
            2,
            3,
            usize::MAX,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
            16,
            17,
            18,
            19,
            u32::MAX as usize,
            20,
            21,
        ],
        renderer: [
            ("cpu_atlas", 0),
            ("gpu_atlas", 83_886_080),
            ("buffers", 22),
            ("depth", 23),
            ("escaped\"\\\n", usize::MAX),
        ],
    };
    let actual: serde_json::Value = serde_json::from_str(&snapshot.json()).unwrap();
    assert_eq!(
        actual,
        serde_json::json!({
            "backend": "webgl2",
            "client_references": 2,
            "terrain_chunks": 3,
            "terrain_bytes": u32::MAX,
            "height_probes": 5,
            "height_candidates": 6,
            "terrain_requests": 7,
            "scene_bytes": 8,
            "render_frame_bytes": 9,
            "fog_bytes": 10,
            "units": 11,
            "unit_bytes": 12,
            "interpolation_bytes": 13,
            "buildings_and_ghosts": 14,
            "animals": 15,
            "projectiles": 16,
            "resource_entries": 17,
            "resource_bytes": 18,
            "stump_effects": 19,
            "outgoing_message_bytes": u32::MAX,
            "control_group_members": 20,
            "move_queue_bytes": 21,
            "renderer": [
                ["cpu_atlas", 0],
                ["gpu_atlas", 83_886_080],
                ["buffers", 22],
                ["depth", 23],
                ["escaped\"\\\n", u32::MAX],
            ],
        })
    );
}
