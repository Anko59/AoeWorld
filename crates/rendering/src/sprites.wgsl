struct Sprite {
    position: vec2<f32>,
    radius: vec2<f32>,
    color: vec4<f32>,
    uv: vec4<f32>,
    depths: vec4<f32>,
    terrain_blend: array<vec4<f32>, 2>,
    pages: vec4<u32>,
};
@group(0) @binding(0) var<storage, read> sprites: array<Sprite>;
@group(0) @binding(1) var sprite_atlas: texture_2d_array<f32>;
@group(0) @binding(2) var sprite_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) solid: u32,
    @location(3) @interpolate(flat) tint_kind: u32,
    @location(4) uv2: vec2<f32>,
    @location(5) uv3: vec2<f32>,
    @location(6) weights: vec3<f32>,
    @location(7) @interpolate(flat) pages: vec4<u32>,
    @location(8) @interpolate(flat) rect: vec4<f32>,
    @location(9) @interpolate(flat) rect2: vec4<f32>,
    @location(10) @interpolate(flat) rect3: vec4<f32>,
};

fn terrain_uv(mode: u32, corner: u32) -> vec2<f32> {
    switch mode {
        case 0u: {
            if corner == 0u { return vec2<f32>(0.5, 0.0); }
            if corner == 1u { return vec2<f32>(1.0, 0.5); }
            return vec2<f32>(0.5, 1.0);
        }
        case 1u: {
            if corner == 0u { return vec2<f32>(0.5, 0.0); }
            if corner == 1u { return vec2<f32>(0.5, 1.0); }
            return vec2<f32>(0.0, 0.5);
        }
        case 2u: {
            if corner == 0u { return vec2<f32>(0.5, 0.0); }
            if corner == 1u { return vec2<f32>(1.0, 0.5); }
            return vec2<f32>(0.0, 0.5);
        }
        case 3u: {
            if corner == 0u { return vec2<f32>(1.0, 0.5); }
            if corner == 1u { return vec2<f32>(0.5, 1.0); }
            return vec2<f32>(0.0, 0.5);
        }
        case 4u: {
            if corner == 0u { return vec2<f32>(0.3, 0.3); }
            if corner == 1u { return vec2<f32>(0.7, 0.3); }
            return vec2<f32>(0.7, 0.7);
        }
        case 5u: {
            if corner == 0u { return vec2<f32>(0.3, 0.3); }
            if corner == 1u { return vec2<f32>(0.7, 0.7); }
            return vec2<f32>(0.3, 0.7);
        }
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

fn terrain_atlas_uv(rect: vec4<f32>, local: vec2<f32>) -> vec2<f32> {
    // A 97×49 frame describes 96×48 intervals: endpoints are texel centres,
    // never the next packed rectangle or the transparent atlas gutter.
    let pixel = vec2<f32>(1.0) / vec2<f32>(textureDimensions(sprite_atlas));
    return rect.xy + pixel * 0.5 + local * max(rect.zw - pixel, vec2<f32>(0.0));
}

fn terrain_tint(kind: u32) -> vec3<f32> {
    switch kind {
        case 1u: { return vec3<f32>(0.92, 0.92, 0.92); }
        case 2u: { return vec3<f32>(0.78, 0.78, 0.78); }
        case 3u: { return vec3<f32>(0.72, 0.72, 0.72); }
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
    out.pages = sprite.pages;
    out.rect = sprite.uv;
    out.rect2 = sprite.terrain_blend[0];
    out.rect3 = sprite.terrain_blend[1];
    out.uv2 = vec2<f32>(0.0);
    out.uv3 = vec2<f32>(0.0);
    out.weights = vec3<f32>(1.0, 0.0, 0.0);
    let is_surface = sprite.color.w < 0.0;
    if is_surface {
        let surface_points = array<vec2<f32>, 3>(sprite.position, sprite.radius, sprite.color.xy);
        let corner = min(vertex, 2u);
        out.clip = vec4<f32>(surface_points[corner], sprite.depths[corner], 1.0);
        if sprite.color.w == -2.0 {
            out.color = vec4<f32>(sprite.uv.xyz, 1.0);
            out.uv = vec2<f32>(0.0);
            out.solid = 2u;
            out.tint_kind = 0u;
        } else {
            let code = u32(sprite.color.z);
            let atlas_uv = terrain_uv(code % 8u, corner);
            out.uv = terrain_atlas_uv(sprite.uv, atlas_uv);
            if (sprite.pages.w & 1073741824u) != 0u { out.uv = atlas_uv; }
            out.color = vec4<f32>(terrain_tint(code / 8u), 1.0);
            out.solid = 1u;
            out.tint_kind = code / 8u;
            if sprite.color.w == -3.0 {
                out.uv2 = terrain_atlas_uv(sprite.terrain_blend[0], atlas_uv);
                out.uv3 = terrain_atlas_uv(sprite.terrain_blend[1], atlas_uv);
                let weights = array<vec3<f32>, 3>(
                    vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
                out.weights = weights[corner];
                if (sprite.pages.w & 1u) != 0u {
                    var floor_strength = f32(min((sprite.pages.w >> 4u) & 1023u, 1000u)) / 1000.0;
                    if (sprite.pages.w & 536870912u) != 0u {
                        floor_strength = f32((u32(sprite.depths.w) >> (corner * 8u)) & 255u) / 255.0;
                    }
                    out.weights = vec3<f32>(1.0 - floor_strength, floor_strength, 0.0);
                }
                out.solid = 3u;
            }
        }
    } else {
        out.clip = vec4<f32>(
            sprite.position + corners[vertex] * sprite.radius,
            sprite.depths.x,
            1.0,
        );
        out.uv = sprite.uv.xy + vec2<f32>((corners[vertex].x + 1.0) * 0.5, (1.0 - corners[vertex].y) * 0.5) * sprite.uv.zw;
        out.color = sprite.color;
        out.solid = 0u;
        out.tint_kind = 0u;
    }
    return out;
}

// Terrain only. Keep nearest coverage; every quadrant stays in this rectangle.
fn terrain_sample(uv: vec2<f32>, page: u32, rect: vec4<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let center = textureSampleLevel(sprite_atlas, sprite_sampler, uv, i32(page), 0.0);
    let size = vec2<f32>(textureDimensions(sprite_atlas));
    let px = dx * size;
    let py = dy * size;
    let footprint = max(dot(px, px), dot(py, py));
    if !(all(abs(rect) <= vec4<f32>(3.402823e38)) && all(abs(uv) <= vec2<f32>(3.402823e38)) && all(abs(dx) <= vec2<f32>(3.402823e38)) && all(abs(dy) <= vec2<f32>(3.402823e38)) && footprint > 1.5625 && footprint <= 3.402823e38) { return center; }
    let lo = rect.xy + 0.5 / size;
    let hi = lo + max(rect.zw - 1.0 / size, vec2<f32>(0.0));
    let offsets = array<vec2<f32>, 4>(-dx-dy, -dx+dy, dx-dy, dx+dy);
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        let tap = floor(textureSampleLevel(sprite_atlas, sprite_sampler, clamp(uv + 0.25 * offsets[i], lo, hi), i32(page), 0.0) * 255.0 + 0.5);
        sum += vec4<f32>(tap.rgb * tap.a, tap.a);
    }
    if sum.a == 0.0 { return center; }
    return vec4<f32>(floor(sum.rgb / sum.a + 0.5) / 255.0, center.a);
}

// CPU-qualified TLUT capability is rechecked against the currently bound atlas.
// All lookup words are raw little-endian RGBA8, never colour-filtered.
struct WorldGroup { range: vec2<u32>, grid: vec2<u32>, kind: u32 };
struct WorldFrame { rect: vec4<f32>, page: u32 };
fn lookup_word(x: u32) -> u32 {
    let b = vec4<u32>(floor(textureLoad(sprite_atlas, vec2<i32>(i32(x), 1), 2, 0) * 255.0 + 0.5));
    return b.x | (b.y << 8u) | (b.z << 16u) | (b.w << 24u);
}
fn lookup_group(id: u32) -> WorldGroup {
    let a = lookup_word(4u + 3u * id); let b = lookup_word(5u + 3u * id);
    return WorldGroup(vec2<u32>(a & 65535u, a >> 16u), vec2<u32>(b & 65535u, b >> 16u), lookup_word(6u + 3u * id));
}
fn world_group_valid(g: WorldGroup) -> bool {
    return g.range.y > 0u && g.range.x + g.range.y <= 672u &&
        ((g.kind == 1u && all(g.grid > vec2<u32>(0u)) && g.grid.x * g.grid.y == g.range.y) ||
         (g.kind == 2u && all(g.grid == vec2<u32>(0u))));
}
fn euclidean_phase(x: i32, period: u32) -> u32 {
    let p = i32(period); return u32((x % p + p) % p);
}
// Shared 16-bit fractional phase resolves barycentric +/-ULP owner seams.
// This is a coordinate precision contract, not alpha smoothing or filtering.
fn world_phase(q: vec2<f32>) -> vec2<f32> {
    return floor(q * 65536.0 + 0.5) / 65536.0;
}
fn world_frame(g: WorldGroup, origin: vec2<u32>, q: vec2<f32>) -> WorldFrame {
    // Explicit unsigned addition/multiplication wrap, then signed interpretation.
    let owner = bitcast<vec2<i32>>(origin + bitcast<vec2<u32>>(vec2<i32>(floor(world_phase(q)))));
    var local: u32;
    if g.kind == 1u {
        local = euclidean_phase(owner.x, g.grid.x) * g.grid.y +
            (g.grid.y - euclidean_phase(owner.y, g.grid.y)) % g.grid.y;
    } else {
        let h = bitcast<u32>(owner.x) * 7u + bitcast<u32>(owner.y) * 13u;
        local = select(h, 0u - h, bitcast<i32>(h) < 0) % g.range.y;
    }
    let first = 32u + 3u * (g.range.x + local);
    let a = lookup_word(first); let b = lookup_word(first + 1u);
    let rect = vec4<f32>(f32(a & 65535u), f32(a >> 16u), f32(b & 65535u), f32(b >> 16u));
    return WorldFrame(rect / vec4<f32>(vec2<f32>(textureDimensions(sprite_atlas)), vec2<f32>(textureDimensions(sprite_atlas))), lookup_word(first + 2u) & 255u);
}
fn world_diamond(q: vec2<f32>) -> vec2<f32> {
    let r = fract(world_phase(q)); return vec2<f32>(0.5 + 0.5 * (r.x - r.y), 0.5 * (r.x + r.y));
}
fn diamond_delta(q: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(0.5 * (q.x - q.y), 0.5 * (q.x + q.y));
}
fn world_tap(g: WorldGroup, origin: vec2<u32>, q: vec2<f32>) -> vec4<f32> {
    let f = world_frame(g, origin, q);
    return textureSampleLevel(sprite_atlas, sprite_sampler, terrain_atlas_uv(f.rect, world_diamond(q)), i32(f.page), 0.0);
}
fn world_sample(g: WorldGroup, origin: vec2<u32>, q: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let f = world_frame(g, origin, q);
    let center = textureSampleLevel(sprite_atlas, sprite_sampler, terrain_atlas_uv(f.rect, world_diamond(q)), i32(f.page), 0.0);
    let intervals = max(f.rect.zw * vec2<f32>(textureDimensions(sprite_atlas)) - 1.0, vec2<f32>(0.0));
    let px = diamond_delta(dx) * intervals; let py = diamond_delta(dy) * intervals;
    let footprint = max(dot(px, px), dot(py, py));
    if !(all(abs(q) <= vec2<f32>(3.402823e38)) && all(abs(dx) <= vec2<f32>(3.402823e38)) && all(abs(dy) <= vec2<f32>(3.402823e38)) && footprint > 1.5625 && footprint <= 3.402823e38) { return center; }
    let offsets = array<vec2<f32>, 4>(-dx-dy, -dx+dy, dx-dy, dx+dy);
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        // Every tap resolves its own world owner, native rectangle and page.
        let tap = floor(world_tap(g, origin, q + 0.25 * offsets[i]) * 255.0 + 0.5);
        sum += vec4<f32>(tap.rgb * tap.a, tap.a);
    }
    if sum.a == 0.0 { return center; }
    return vec4<f32>(floor(sum.rgb / sum.a + 0.5) / 255.0, center.a);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Derivatives are uniform and evaluated before any branch/discard.
    let dx = dpdx(in.uv); let dy = dpdy(in.uv);
    let dx2 = dpdx(in.uv2); let dy2 = dpdy(in.uv2);
    let dx3 = dpdx(in.uv3); let dy3 = dpdy(in.uv3);
    if in.solid == 2u {
        return in.color;
    }
    var texel: vec4<f32>;
    if (in.pages.w & 1073741824u) != 0u && (in.solid == 1u || in.solid == 3u) {
        let primary = u32(in.rect3.z) & 7u; let secondary = u32(in.rect3.w) & 7u;
        var valid = lookup_word(0u) == 0x54554c54u && lookup_word(1u) == 0x07030001u && lookup_word(3u) == in.pages.z;
        var g: WorldGroup; var g2: WorldGroup;
        if valid {
            g = lookup_group(primary); valid = primary < 7u && world_group_valid(g);
            if in.solid == 3u && secondary != primary {
                g2 = lookup_group(secondary); valid = valid && secondary < 7u && world_group_valid(g2);
            }
        }
        if valid {
            let q = vec2<f32>(in.uv.x + in.uv.y - 0.5, in.uv.y - in.uv.x + 0.5) * in.rect3.xy;
            let qx = vec2<f32>(dx.x + dx.y, dx.y - dx.x) * in.rect3.xy;
            let qy = vec2<f32>(dy.x + dy.y, dy.y - dy.x) * in.rect3.xy;
            texel = world_sample(g, in.pages.xy, q, qx, qy);
            if in.solid == 3u && secondary != primary {
                texel = texel * in.weights.x + world_sample(g2, in.pages.xy, q, qx, qy) * in.weights.y;
            }
        } else {
            // Stale/missing/replaced table: original stretched native rectangle.
            let span = max(in.rect.zw - 1.0 / vec2<f32>(textureDimensions(sprite_atlas)), vec2<f32>(0.0));
            texel = terrain_sample(terrain_atlas_uv(in.rect, in.uv), u32(in.rect3.z) >> 3u, in.rect, dx * span, dy * span);
            if in.solid == 3u {
                let span2 = max(in.rect2.zw - 1.0 / vec2<f32>(textureDimensions(sprite_atlas)), vec2<f32>(0.0));
                texel = texel * in.weights.x + terrain_sample(terrain_atlas_uv(in.rect2, in.uv), u32(in.rect3.w) >> 3u, in.rect2, dx * span2, dy * span2) * in.weights.y;
            }
        }
    } else if in.solid == 0u || (in.pages.w & 1u) == 0u {
        texel = textureSampleLevel(sprite_atlas, sprite_sampler, in.uv, i32(in.pages.x), 0.0);
    } else {
        texel = terrain_sample(in.uv, in.pages.x, in.rect, dx, dy);
    }
    if in.solid == 3u && (in.pages.w & 1073741824u) == 0u {
        if (in.pages.w & 1u) != 0u {
            texel = texel * in.weights.x
                + terrain_sample(in.uv2, in.pages.y, in.rect2, dx2, dy2) * in.weights.y
                + texel * in.weights.z;
        } else {
            texel = texel * in.weights.x
                + textureSampleLevel(sprite_atlas, sprite_sampler, in.uv2, i32(in.pages.y), 0.0) * in.weights.y
                + textureSampleLevel(sprite_atlas, sprite_sampler, in.uv3, i32(in.pages.z), 0.0) * in.weights.z;
        }
    }
    if texel.a <= 0.0 {
        discard;
    }
    if in.tint_kind == 4u {
        let water = vec3<f32>(0.14901961, 0.44313726, 0.74509805);
        return vec4<f32>(mix(texel.rgb, water, 0.14), texel.a);
    }
    let ramp = in.tint_kind >= 21u && in.tint_kind <= 26u;
    let kind = select(in.tint_kind, in.tint_kind - 16u, ramp);
    if (kind >= 5u && kind <= 10u) || kind == 12u {
        let detail = dot(texel.rgb, vec3<f32>(1.0 / 3.0));
        var base = vec3<f32>(0.18);
        var amount = 0.55;
        var shade = 1.0;
        switch kind {
            case 6u: { base = vec3<f32>(0.78, 0.79, 0.78); amount = 0.12; }
            case 7u: { shade = 0.72; }
            case 12u: { shade = 0.78; }
            case 8u: { base = vec3<f32>(0.42, 0.57, 0.65); amount = 0.20; }
            case 9u: { base = vec3<f32>(0.10, 0.08, 0.05); amount = 0.35; }
            case 10u: { base = vec3<f32>(0.22, 0.36, 0.33); amount = 0.25; }
            default: {}
        }
        shade *= select(1.0, 0.92, ramp);
        return vec4<f32>((base + detail * amount) * shade, texel.a);
    }
    if (in.pages.w & 1u) != 0u && in.tint_kind <= 3u {
        var scales = vec3<f32>(1000.0);
        switch (in.pages.w >> 1u) & 7u {
            case 0u: { scales = vec3<f32>(990.0, 1000.0, 970.0); }
            case 1u: { scales = vec3<f32>(950.0, 1000.0, 980.0); }
            case 2u: { scales = vec3<f32>(940.0, 1000.0, 930.0); }
            case 3u: { scales = vec3<f32>(1040.0, 980.0, 880.0); }
            case 4u: { scales = vec3<f32>(1030.0, 1000.0, 900.0); }
            default: {}
        }
        let canopy = f32(min((in.pages.w >> 14u) & 1023u, 1000u)) / 1000.0;
        let factor = in.color.rgb * (scales / 1000.0) * (1.0 - 0.12 * canopy);
        return vec4<f32>(floor(clamp(texel.rgb * 255.0 * factor, vec3<f32>(0.0), vec3<f32>(255.0)) + 0.5) / 255.0, texel.a);
    }
    return texel * in.color;
}
