// R8Uint: bit 0 is tall-object coverage, bit 1 is hovered-body coverage.
// The contour pass reads it after that attachment has finished, reusing depth.
@group(1) @binding(0) var occluder_mask: texture_2d<u32>;

struct WorldFragment {
    @location(0) color: vec4<f32>,
    @location(1) occluder: u32,
};
// Same seven attributes and 112-byte packet as the WebGL2 instance stream.
struct InstanceInput {
    @location(0) position_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) depths: vec4<f32>,
    @location(4) blend0: vec4<f32>,
    @location(5) blend1: vec4<f32>,
    @location(6) pages: vec4<u32>,
};
fn instance_sprite(input: InstanceInput) -> Sprite {
    return Sprite(input.position_radius.xy, input.position_radius.zw, input.color,
        input.uv, input.depths, array<vec4<f32>, 2>(input.blend0, input.blend1), input.pages);
}
@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, input: InstanceInput) -> VertexOutput {
    return sprite_vertex(vertex, instance_sprite(input));
}
@fragment
fn fs_main(in: VertexOutput) -> WorldFragment {
    let color = world_colour(in);
    let tall = in.solid == 0u && (in.pages.w & 4u) != 0u && color.a >= 0.8;
    let hovered = in.solid == 0u && (in.pages.w & 48u) != 0u && color.a >= 0.8;
    return WorldFragment(color, select(0u, 1u, tall) | select(0u, 2u, hovered));
}
@vertex
fn vs_unit_contour(@builtin(vertex_index) vertex: u32, input: InstanceInput) -> VertexOutput {
    let sprite = instance_sprite(input);
    if sprite.color.w >= 0.0 && (sprite.pages.w & 56u) != 0u {
        return sprite_vertex(vertex, sprite);
    }
    var out: VertexOutput;
    out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    return out;
}
fn contour_alpha(uv: vec2<f32>, rect: vec4<f32>, page: u32) -> f32 {
    let lo = min(rect.xy, rect.xy + rect.zw);
    let hi = max(rect.xy, rect.xy + rect.zw);
    if any(uv < lo) || any(uv >= hi) { return 0.0; }
    return textureSampleLevel(sprite_atlas, sprite_sampler, uv, i32(page), 0.0).a;
}
fn edge_alpha(in: VertexOutput, step_x: vec2<f32>, step_y: vec2<f32>, width: f32) -> f32 {
    let dx = step_x * width;
    let dy = step_y * width;
    let x = min(contour_alpha(in.uv - dx, in.rect, in.pages.x), contour_alpha(in.uv + dx, in.rect, in.pages.x));
    let y = min(contour_alpha(in.uv - dy, in.rect, in.pages.x), contour_alpha(in.uv + dy, in.rect, in.pages.x));
    return 1.0 - min(x, y);
}
@fragment
fn fs_unit_contour(in: VertexOutput) -> @location(0) vec4<f32> {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let mask = textureLoad(occluder_mask, vec2<i32>(in.clip.xy), 0).r;
    let alpha = contour_alpha(in.uv, in.rect, in.pages.x);
    if alpha < 0.8 { discard; }
    if (in.pages.w & 48u) != 0u {
        // Only the final visible body may receive a hover rim. Shadows, holes,
        // nearer objects and fog remain governed by the world coverage pass.
        if (mask & 2u) == 0u { discard; }
        var opacity = edge_alpha(in, dx, dy, 1.25) * 0.65;
        if (in.pages.w & 32u) != 0u {
            opacity = edge_alpha(in, dx, dy, 1.25) * 0.4 + edge_alpha(in, dx, dy, 3.0) * 0.22 + edge_alpha(in, dx, dy, 5.0) * 0.12;
        }
        if opacity <= 0.01 { discard; }
        return vec4<f32>(1.0, 0.88, 0.55, opacity);
    }
    if (mask & 1u) == 0u || edge_alpha(in, dx, dy, 1.5) < 0.2 { discard; }
    return vec4<f32>(in.rect2.rgb, 1.0);
}
