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

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let sprite = sprites[instance];
    var out: VertexOutput;
    let is_surface = sprite.uv.w < 0.0;
    if is_surface {
        let points = array<vec2<f32>, 6>(
            sprite.position, sprite.radius, sprite.uv.xy,
            sprite.uv.xy, sprite.uv.xy, sprite.uv.xy);
        out.clip = vec4<f32>(points[vertex], 0.0, 1.0);
        out.uv = vec2<f32>(0.0);
        out.solid = 1u;
    } else {
        out.clip = vec4<f32>(sprite.position + corners[vertex] * sprite.radius, 0.0, 1.0);
        out.uv = sprite.uv.xy + vec2<f32>((corners[vertex].x + 1.0) * 0.5, (1.0 - corners[vertex].y) * 0.5) * sprite.uv.zw;
        out.solid = 0u;
    }
    out.color = sprite.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.solid == 1u {
        return in.color;
    }
    return textureSampleLevel(sprite_atlas, sprite_sampler, in.uv, 0.0) * in.color;
}
