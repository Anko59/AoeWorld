fn terrain_tint(kind: u32) -> vec3<f32> {
    switch kind {
        case 1u: { return vec3<f32>(0.92); }
        case 2u: { return vec3<f32>(0.78); }
        case 3u: { return vec3<f32>(0.72); }
        default: { return vec3<f32>(1.0); }
    }
}

// The high atlas-page byte is unused by ground textures. Vertex interpolation
// softens the black/explored/visible boundary on the existing ground mesh.
fn fog_level(packet: u32, corner: u32) -> f32 {
    if (packet & 2147483648u) == 0u { return 1.0; }
    let levels = array<f32, 4>(0.0, 0.45, 0.725, 1.0);
    return levels[(packet >> (24u + corner * 2u)) & 3u];
}
