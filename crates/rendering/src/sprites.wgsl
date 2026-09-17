struct Sprite {
    position: vec2<f32>,
    radius: vec2<f32>,
    color: vec4<f32>,
};
@group(0) @binding(0) var<storage, read> sprites: array<Sprite>;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let sprite = sprites[instance];
    var out: VertexOutput;
    out.clip = vec4<f32>(sprite.position + corners[vertex] * sprite.radius, 0.0, 1.0);
    out.color = sprite.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
