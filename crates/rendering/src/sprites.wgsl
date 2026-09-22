struct Sprite {
    position: vec2<f32>,
    radius: vec2<f32>,
    color: vec4<f32>,
    uv: vec4<f32>,
};
@group(0) @binding(0) var<storage, read> sprites: array<Sprite>;
@group(0) @binding(1) var sprite_atlas: texture_2d<f32>;
@group(0) @binding(2) var sprite_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) solid: u32,
};

fn terrain_uv(mode: u32, corner: u32) -> vec2<f32> {
    switch mode {
        case 0u, 4u: {
            if corner == 0u { return vec2<f32>(0.0, 0.0); }
            if corner == 1u { return vec2<f32>(1.0, 0.0); }
            return vec2<f32>(1.0, 1.0);
        }
        case 1u, 5u: {
            if corner == 0u { return vec2<f32>(0.0, 0.0); }
            if corner == 1u { return vec2<f32>(1.0, 1.0); }
            return vec2<f32>(0.0, 1.0);
        }
        case 2u: {
            if corner == 0u { return vec2<f32>(0.0, 0.0); }
            if corner == 1u { return vec2<f32>(1.0, 0.0); }
            return vec2<f32>(0.0, 1.0);
        }
        case 3u: {
            if corner == 0u { return vec2<f32>(1.0, 0.0); }
            if corner == 1u { return vec2<f32>(1.0, 1.0); }
            return vec2<f32>(0.0, 1.0);
        }
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

fn terrain_tint(kind: u32) -> vec3<f32> {
    switch kind {
        case 1u: { return vec3<f32>(0.92, 0.92, 0.92); }
        case 2u: { return vec3<f32>(0.78, 0.78, 0.78); }
        case 3u: { return vec3<f32>(0.72, 0.72, 0.72); }
        case 4u: { return vec3<f32>(0.88, 0.94, 1.0); }
        default: { return vec3<f32>(1.0, 1.0, 1.0); }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let sprite = sprites[instance];
    var out: VertexOutput;
    let is_surface = sprite.color.w < 0.0;
    if is_surface {
        let surface_points = array<vec2<f32>, 3>(sprite.position, sprite.radius, sprite.color.xy);
        let corner = min(vertex, 2u);
        out.clip = vec4<f32>(surface_points[corner], 0.0, 1.0);
        if sprite.color.w < -1.5 {
            out.color = vec4<f32>(sprite.uv.xyz, 1.0);
            out.uv = vec2<f32>(0.0);
            out.solid = 2u;
        } else {
            let code = u32(sprite.color.z);
            let atlas_uv = terrain_uv(code % 8u, corner);
            out.uv = sprite.uv.xy + atlas_uv * sprite.uv.zw;
            out.color = vec4<f32>(terrain_tint(code / 8u), 1.0);
            out.solid = 0u;
        }
    } else {
        out.clip = vec4<f32>(sprite.position + corners[vertex] * sprite.radius, 0.0, 1.0);
        out.uv = sprite.uv.xy + vec2<f32>((corners[vertex].x + 1.0) * 0.5, (1.0 - corners[vertex].y) * 0.5) * sprite.uv.zw;
        out.color = sprite.color;
        out.solid = 0u;
    }
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.solid == 2u {
        return in.color;
    }
    return textureSampleLevel(sprite_atlas, sprite_sampler, in.uv, 0.0) * in.color;
}
