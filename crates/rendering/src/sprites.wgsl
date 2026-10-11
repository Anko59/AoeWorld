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

// Terrain frame directory in rows 0-1 of page 0 (surface_mesh/directory.rs).
fn directory(entry: u32) -> vec4<f32> {
    return floor(textureLoad(sprite_atlas, vec2<u32>(entry % 2048u, entry / 2048u), 0u, 0u) * 255.0 + 0.5);
}

// One scene material's first frame entry, frame count, sheet columns and rows.
fn sheet(material: u32) -> vec4<f32> {
    let a = directory(1u + 2u * material);
    let b = directory(2u + 2u * material);
    return vec4<f32>(a.x + a.y * 256.0, a.z + a.w * 256.0, b.x + b.y * 256.0, b.z + b.w * 256.0);
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
            out.color = sprite.uv;
            out.uv = vec2<f32>(0.0);
            out.solid = 2u;
            out.tint_kind = 0u;
        } else {
            let code = u32(sprite.color.z);
            let atlas_uv = terrain_uv(code % 8u, corner);
            // Mode 6 spans several world tiles: corners travel in tile units and
            // each fragment resolves the frame its own world tile owns.
            let tiled = code % 8u == 6u;
            out.uv = terrain_atlas_uv(sprite.uv, atlas_uv);
            out.color = vec4<f32>(terrain_tint(code / 8u), 1.0);
            out.solid = 1u;
            out.tint_kind = code / 8u;
            if tiled {
                let locals = array<vec2<f32>, 3>(sprite.uv.xy, sprite.uv.zw, sprite.terrain_blend[0].xy);
                out.uv = locals[corner];
                out.rect = sheet(sprite.pages.z & 255u);
                out.solid = 5u;
            }
            if sprite.color.w == -3.0 {
                if tiled {
                    out.rect2 = sheet((sprite.pages.z >> 8u) & 255u);
                    out.rect3 = sheet((sprite.pages.z >> 16u) & 255u);
                } else {
                    out.uv2 = terrain_atlas_uv(sprite.terrain_blend[0], atlas_uv);
                    out.uv3 = terrain_atlas_uv(sprite.terrain_blend[1], atlas_uv);
                }
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
                out.solid |= 2u;
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

struct Layer {
    uv: vec2<f32>,
    page: u32,
    rect: vec4<f32>,
    dx: vec2<f32>,
    dy: vec2<f32>,
};

// World-anchored layer: this fragment's world tile selects its native frame.
// Euclidean remainder over the whole signed range, in unsigned arithmetic.
fn wrap(value: i32, period: u32) -> u32 {
    return ((bitcast<u32>(value) ^ 2147483648u) % period + period - 2147483648u % period) % period;
}

fn tiled_layer(header: vec4<f32>, tile: vec2<i32>, local: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> Layer {
    let columns = u32(header.z);
    let rows = u32(header.w);
    let hash = bitcast<u32>(tile.x) * 7u + bitcast<u32>(tile.y) * 13u;
    var index = select(hash, ~hash + 1u, hash > 2147483648u) % u32(max(header.y, 1.0));
    if columns > 0u && rows > 0u {
        index = wrap(tile.x, columns) * rows + (rows - wrap(tile.y, rows)) % rows;
    }
    let entry = u32(header.x) + 2u * index;
    let a = directory(entry);
    let b = directory(entry + 1u);
    let high = u32(b.z);
    let size = vec2<f32>(textureDimensions(sprite_atlas));
    var layer: Layer;
    layer.rect = vec4<f32>(a.x + a.y * 256.0, a.z + a.w * 256.0, b.x + f32(high & 15u) * 256.0, b.y + f32(high >> 4u) * 256.0) / size.xyxy;
    layer.page = u32(b.w);
    let span = max(layer.rect.zw - 1.0 / size, vec2<f32>(0.0));
    layer.uv = layer.rect.xy + 0.5 / size + local * span;
    layer.dx = gx * span;
    layer.dy = gy * span;
    return layer;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Derivatives are uniform and evaluated before any branch/discard.
    var first = Layer(in.uv, in.pages.x, in.rect, dpdx(in.uv), dpdy(in.uv));
    var second = Layer(in.uv2, in.pages.y, in.rect2, dpdx(in.uv2), dpdy(in.uv2));
    var third = Layer(in.uv3, in.pages.z, in.rect3, dpdx(in.uv3), dpdy(in.uv3));
    if in.solid == 2u {
        return in.color;
    }
    let blend = (in.solid & 3u) == 3u;
    let landscape = (in.pages.w & 1u) != 0u;
    if (in.solid & 4u) != 0u {
        if in.rect.y < 1.0 {
            discard;
        }
        let cell = floor(in.uv);
        let inside = in.uv - cell;
        let tile = vec2<i32>(bitcast<i32>(in.pages.x), bitcast<i32>(in.pages.y)) + vec2<i32>(cell);
        let local = vec2<f32>(0.5 + 0.5 * (inside.x - inside.y), 0.5 * (inside.x + inside.y));
        let gx = vec2<f32>(0.5 * (first.dx.x - first.dx.y), 0.5 * (first.dx.x + first.dx.y));
        let gy = vec2<f32>(0.5 * (first.dy.x - first.dy.y), 0.5 * (first.dy.x + first.dy.y));
        first = tiled_layer(in.rect, tile, local, gx, gy);
        if blend {
            second = tiled_layer(in.rect2, tile, local, gx, gy);
            if !landscape {
                third = tiled_layer(in.rect3, tile, local, gx, gy);
            }
        }
    }
    var texel: vec4<f32>;
    if in.solid == 0u || !landscape {
        texel = textureSampleLevel(sprite_atlas, sprite_sampler, first.uv, i32(first.page), 0.0);
    } else {
        texel = terrain_sample(first.uv, first.page, first.rect, first.dx, first.dy);
    }
    if blend {
        if landscape {
            texel = texel * in.weights.x
                + terrain_sample(second.uv, second.page, second.rect, second.dx, second.dy) * in.weights.y
                + texel * in.weights.z;
        } else {
            texel = texel * in.weights.x
                + textureSampleLevel(sprite_atlas, sprite_sampler, second.uv, i32(second.page), 0.0) * in.weights.y
                + textureSampleLevel(sprite_atlas, sprite_sampler, third.uv, i32(third.page), 0.0) * in.weights.z;
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
