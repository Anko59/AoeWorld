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
    @location(11) shade: f32,
    @location(12) fog: f32,
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
    let pixel = vec2<f32>(1.0) / vec2<f32>(textureDimensions(sprite_atlas));
    return rect.xy + pixel * 0.5 + local * max(rect.zw - pixel, vec2<f32>(0.0));
}
fn directory(entry: u32) -> vec4<f32> {
    return floor(textureLoad(sprite_atlas, vec2<u32>(entry % 2048u, entry / 2048u), 0u, 0u) * 255.0 + 0.5);
}

fn sheet(material: u32) -> vec4<f32> {
    let a = directory(1u + 2u * material);
    let b = directory(2u + 2u * material);
    return vec4<f32>(a.x + a.y * 256.0, a.z + a.w * 256.0, b.x + b.y * 256.0, b.z + b.w * 256.0);
}

fn sprite_vertex(vertex: u32, instance: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let sprite = sprites[instance];
    var out: VertexOutput;
    out.fog = 1.0;
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
        out.fog = fog_level(sprite.pages.z, corner);
        out.pages.z = sprite.pages.z & 16777215u;
        out.clip = vec4<f32>(surface_points[corner], sprite.depths[corner], 1.0);
        if sprite.color.w == -2.0 {
            out.color = sprite.uv;
            out.uv = vec2<f32>(0.0);
            out.solid = 2u;
            out.tint_kind = 0u;
            let vertex_code = (u32(sprite.color.z) >> (12u + 4u * corner)) & 15u;
            out.shade = f32(vertex_code) - select(0.0, 16.0, vertex_code >= 8u);
        } else {
            let code = u32(sprite.color.z);
            let atlas_uv = terrain_uv(code % 8u, corner);
            let tiled = code % 8u == 6u;
            out.uv = terrain_atlas_uv(sprite.uv, atlas_uv);
            out.color = vec4<f32>(terrain_tint((code / 8u) % 32u), 1.0);
            out.solid = 1u;
            out.tint_kind = code / 8u;
            let vertex_code = (code >> (12u + 4u * corner)) & 15u;
            out.shade = f32(vertex_code) - select(0.0, 16.0, vertex_code >= 8u);
            if tiled {
                let locals = array<vec2<f32>, 3>(sprite.uv.xy, sprite.uv.zw, sprite.terrain_blend[0].xy);
                out.uv = locals[corner];
                out.rect = sheet(sprite.pages.z & 255u);
                out.solid = 5u;
            }
            if sprite.color.w == -3.0 {
                if tiled {
                    out.rect2 = sheet((sprite.pages.z >> 8u) & 255u);
                    if u32(sprite.terrain_blend[0].z) != 3u {
                        out.rect3 = sheet((sprite.pages.z >> 16u) & 255u);
                    }
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
                    // #if mixing
                    let mode = u32(sprite.terrain_blend[0].z);
                    if tiled && mode != 0u {
                        let share = u32(sprite.terrain_blend[1][corner]);
                        out.weights = vec3<f32>(f32(share & 255u) / 255.0, f32(share >> 8u) / 255.0, floor_strength);
                        if mode == 3u {
                            out.rect = vec4<f32>(sprite.terrain_blend[0].w, sprite.terrain_blend[1].w, f32((sprite.pages.z >> 16u) & 255u), 0.0);
                            out.weights = vec3<f32>(weights[corner].xy, floor_strength);
                        }
                        out.solid |= 8u * mode;
                    }
                    // #endif
                }
                out.solid |= 2u;
            }
        }
    } else {
        // Selection ellipses carry the depth of their top edge in x and of
        // their bottom edge in y, so they follow the ground under the stroke.
        var quad_depth = sprite.depths.x;
        if sprite.pages.w == 2u && corners[vertex].y < 0.0 {
            quad_depth = sprite.depths.y;
        }
        out.clip = vec4<f32>(
            sprite.position + corners[vertex] * sprite.radius,
            quad_depth,
            1.0,
        );
        out.uv = sprite.uv.xy + vec2<f32>((corners[vertex].x + 1.0) * 0.5, (1.0 - corners[vertex].y) * 0.5) * sprite.uv.zw;
        out.color = sprite.color;
        out.solid = 0u;
        if sprite.pages.w == 2u {
            out.solid = 64u;
            out.uv = corners[vertex] * (sprite.uv.xy + vec2<f32>(1.0));
        }
        out.tint_kind = 0u;
    }
    return out;
}

struct Layer {
    uv: vec2<f32>,
    page: u32,
    rect: vec4<f32>,
    dx: vec2<f32>,
    dy: vec2<f32>,
};

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

fn ground(rgb: vec3<f32>, kind: u32) -> vec3<f32> {
    var base = vec3<f32>(0.18);
    var amount = 0.55;
    switch kind {
        case 5u, 7u, 12u: {}
        case 6u: { base = vec3<f32>(0.82, 0.835, 0.85); amount = 0.14; }
        case 8u: { base = vec3<f32>(0.42, 0.57, 0.65); amount = 0.20; }
        case 9u: { base = vec3<f32>(0.10, 0.08, 0.05); amount = 0.35; }
        case 10u: { base = vec3<f32>(0.22, 0.36, 0.33); amount = 0.25; }
        default: { return rgb; }
    }
    return base + dot(rgb, vec3<f32>(1.0 / 3.0)) * amount;
}

// #if mixing
fn mixed(texel: vec4<f32>, material: u32, appearance: u32, tint: u32) -> vec4<f32> {
    let scene = material & 15u;
    if (appearance & 2147483648u) != 0u && scene == 8u {
        return texel;
    }
    var kind = 0u;
    switch material & 15u {
        case 8u: { kind = 8u; }
        case 9u: { kind = 9u; }
        default: {}
    }
    var color = vec4<f32>(ground(texel.rgb, kind), texel.a);
    if scene == 4u || scene == 7u {
        color = vec4<f32>(color.rgb / terrain_tint(tint), color.a);
    }
    return color;
}

fn lattice(p: vec2<i32>) -> f32 {
    var hash = (bitcast<u32>(p.x) * 2654435761u) ^ (bitcast<u32>(p.y) * 2246822519u);
    hash = (hash ^ (hash >> 15u)) * 739982445u;
    return f32((hash ^ (hash >> 12u)) & 255u) / 255.0;
}

fn value(tile: vec2<i32>, inside: vec2<f32>, cells: i32) -> f32 {
    let scaled = inside * f32(cells);
    let cell = floor(scaled);
    let t = scaled - cell;
    let u = t * t * (3.0 - 2.0 * t);
    let at = tile * cells + vec2<i32>(cell);
    let a = lattice(at);
    let b = lattice(at + vec2<i32>(1, 0));
    let c = lattice(at + vec2<i32>(0, 1));
    let d = lattice(at + vec2<i32>(1, 1));
    let top = a + (b - a) * u.x;
    let bottom = c + (d - c) * u.x;
    return top + (bottom - top) * u.y;
}

fn edge(share: f32, noise: f32) -> f32 {
    if share <= 0.0 { return 0.0; }
    if share >= 1.0 { return 1.0; }
    return clamp((share - 0.5) * 4.0 + 0.5 + (noise - 0.5) * 2.6, 0.0, 1.0);
}

fn shore_edge(share: f32, water: f32, noise: f32) -> f32 {
    if share <= water { return edge(share, 0.5); }
    let cover = edge(water, 0.5);
    return cover + (1.0 - cover) * edge((share - water) / (1.0 - water), noise);
}

fn junction_share(word: u32, material: u32) -> f32 {
    var present = 0u;
    var matching = 0u;
    for (var slot = 0u; slot < 4u; slot++) {
        let cell = (word >> (slot * 4u)) & 15u;
        present += select(0u, 1u, cell != 15u);
        matching += select(0u, 1u, cell == material);
    }
    return f32((matching * 255u + present / 2u) / max(present, 1u)) / 255.0;
}
fn junction_water(word: u32) -> f32 {
    return junction_share(word, 10u) + junction_share(word, 12u);
}
// #endif

fn world_colour(in: VertexOutput) -> vec4<f32> {
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let dx2 = dpdx(in.uv2);
    let dy2 = dpdy(in.uv2);
    let dx3 = dpdx(in.uv3);
    let dy3 = dpdy(in.uv3);
    if in.solid == 64u {
        let q = in.uv / in.rect.xy;
        let distance = abs(dot(q, q) - 1.0) / max(0.0001, 2.0 * length(in.uv / (in.rect.xy * in.rect.xy)));
        if distance > in.rect.z * 0.5 { discard; }
        return in.color;
    }
    if in.solid == 0u {
        if (in.pages.w & 128u) != 0u && dot(abs((in.uv - in.rect.xy) / in.rect.zw - vec2<f32>(0.5)), vec2<f32>(1.0)) > 0.5 { discard; }
        let owner = vec4<f32>(in.rect3.rgb, select(0.0, 1.0, (in.pages.w & 64u) != 0u));
        var texel = textureSampleLevel(sprite_atlas, sprite_sampler, in.uv, i32(in.pages.x), 0.0);
        texel = owner_texel(texel, in.uv, i32(in.pages.x), owner);
        // Quad pages.w bit 0 marks nearest-sampled terrain, not an object.
        if (in.pages.w & 1u) == 0u {
            let average = box_average(in.uv, i32(in.pages.x), in.rect, dx, dy, owner);
            if average.a >= 0.0 { texel = average; }
        }
        if texel.a <= 0.0 { discard; }
        if in.color.r > 1.0 { texel.r = min(texel.r, 1.0 / in.color.r); }
        return texel * in.color;
    }
    if in.solid == 2u { return vec4<f32>(in.color.rgb * (1.0 + in.shade * 0.04) * in.fog, in.color.a); }
    let blend = (in.solid & 3u) == 3u;
    let landscape = (in.pages.w & 1u) != 0u;
    // #if mixing
    let mixing = in.solid >= 8u;
    let junction = in.solid >= 24u;
    // #endif
    let tiled = (in.solid & 4u) != 0u;
    var inside = vec2<f32>(0.0);
    var tile = vec2<i32>(0);
    var local = vec2<f32>(0.0);
    var gx = vec2<f32>(0.0);
    var gy = vec2<f32>(0.0);
    if tiled {
        // #if mixing
        if !junction && in.rect.y < 1.0 { discard; }
        // #else
        if in.rect.y < 1.0 { discard; }
        // #endif
        let cell = floor(in.uv);
        inside = in.uv - cell;
        tile = vec2<i32>(bitcast<i32>(in.pages.x), bitcast<i32>(in.pages.y)) + vec2<i32>(cell);
        local = vec2<f32>(0.5 + 0.5 * (inside.x - inside.y), 0.5 * (inside.x + inside.y));
        gx = vec2<f32>(0.5 * (dx.x - dx.y), 0.5 * (dx.x + dx.y));
        gy = vec2<f32>(0.5 * (dy.x - dy.y), 0.5 * (dy.x + dy.y));
    }
    // #if mixing
    let single = !blend || (landscape && !mixing && (in.weights.y <= 0.0 || in.weights.y >= 1.0));
    // #else
    let single = !blend || (landscape && (in.weights.y <= 0.0 || in.weights.y >= 1.0));
    // #endif
    let bed_only = blend && single && in.weights.y >= 1.0;
    var weights = vec3<f32>(1.0, 0.0, 0.0);
    var count = 1u;
    if blend && !single {
        weights = in.weights;
        count = 3u;
        // #if mixing
        if landscape && !mixing { count = 2u; }
        // #else
        if landscape { count = 2u; }
        // #endif
    }
    if landscape && blend && ((in.pages.w >> 1u) & 7u) == 6u {
        let water = clamp((in.weights.y - 0.5) * 4.0 + 0.5, 0.0, 1.0);
        weights = vec3<f32>(1.0 - water, water, 0.0);
    }
    // #if mixing
    var noise = 0.5;
    if mixing && !junction {
        let middle = in.weights.x + in.weights.y;
        if (in.solid >= 16u || in.weights.z < 1.0) &&
            ((in.weights.x > 0.0 && in.weights.x < 1.0) ||
             (in.solid >= 16u && middle > 0.0 && middle < 1.0)) {
            noise = 0.6 * value(tile, inside, 1) + 0.4 * value(tile, inside, 4);
        }
        let top = (in.pages.z >> 16u) & 255u;
        let mid_material = (in.pages.z >> 8u) & 255u;
        let water = select(0.0, in.weights.x + select(0.0, in.weights.y, mid_material == 10u || mid_material == 12u), top == 10u || top == 12u);
        let over = shore_edge(in.weights.x, water, noise);
        weights = vec3<f32>((1.0 - in.weights.z) * (1.0 - over), in.weights.z, (1.0 - in.weights.z) * over);
        if in.solid >= 16u {
            let mid = shore_edge(middle, water, noise);
            weights = vec3<f32>(1.0 - mid, mid - over, over);
        }
    }
    var ground_count = 0u;
    if junction {
        ground_count = min(u32(in.rect.z), 12u);
        count = ground_count + 1u;
        noise = 0.6 * value(tile, inside, 1) + 0.4 * value(tile, inside, 4);
    }
    let bary = vec3<f32>(in.weights.xy, 1.0 - in.weights.x - in.weights.y);
    var water = 0.0;
    if junction { water = dot(vec3<f32>(junction_water(u32(in.rect3.x)), junction_water(u32(in.rect3.y)), junction_water(u32(in.rect3.z))), bary); }
    var cumulative = 0.0;
    var previous = 0.0;
    // #endif
    var texel = vec4<f32>(0.0);
    for (var index = 0u; index < count; index++) {
        var layer = Layer(in.uv, in.pages.x, in.rect, dx, dy);
        var header = in.rect;
        // #if mixing
        var material = 0u;
        // #endif
        var weight = 1.0;
        if single {
            if bed_only {
                layer = Layer(in.uv2, in.pages.y, in.rect2, dx2, dy2);
                header = in.rect2;
            }
        // #if mixing
        } else if junction {
            if index < ground_count {
                let slot = ground_count - 1u - index;
                let word = u32(in.rect[slot / 6u]);
                material = (word >> ((slot % 6u) * 4u)) & 15u;
                if material == 15u { continue; }
                let shares = vec3<f32>(junction_share(u32(in.rect3.x), material), junction_share(u32(in.rect3.y), material), junction_share(u32(in.rect3.z), material));
                cumulative += dot(shares, bary);
                let threshold = shore_edge(cumulative, water, noise);
                weight = (threshold - previous) * (1.0 - in.weights.z);
                previous = threshold;
                if weight <= 0.0 { continue; }
                material = select(select(select(material, 15u, material == 5u), 16u, material == 6u), 17u, material == 11u);
                header = sheet(material);
            } else {
                weight = in.weights.z;
                header = in.rect2;
                material = (in.pages.z >> 8u) & 255u;
            }
        // #endif
        } else {
            weight = weights[index];
            if index == 1u { layer = Layer(in.uv2, in.pages.y, in.rect2, dx2, dy2); header = in.rect2; }
            if index == 2u { layer = Layer(in.uv3, in.pages.z, in.rect3, dx3, dy3); header = in.rect3; }
            // #if mixing
            material = (in.pages.z >> (index * 8u)) & 255u;
            // #endif
        }
        if weight <= 0.0 { continue; }
        if tiled { layer = tiled_layer(header, tile, local, gx, gy); }
        var sample: vec4<f32>;
        if landscape {
            sample = terrain_sample(layer.uv, layer.page, layer.rect, layer.dx, layer.dy);
        } else {
            sample = terrain_sample(layer.uv, layer.page, layer.rect, vec2<f32>(0.0), vec2<f32>(0.0));
        }
        // #if mixing
        if mixing { sample = mixed(sample, material, in.pages.w, in.tint_kind % 32u); }
        // #endif
        if single { texel = sample; } else { texel += sample * weight; }
    }
    // Packet tint_kind carries wear and slope shade above its five tint bits.
    let worn = shaded(texel, in.color, in.tint_kind % 32u, in.pages.w);
    let darken = vec3<f32>(0.20, 0.32, 0.38) * (f32((in.tint_kind / 32u) % 16u) / 15.0);
    // Slope shade: a signed four-bit code per vertex, interpolated, 4 % a step.
    let lit = 1.0 + in.shade * 0.04;
    return vec4<f32>(worn.rgb * (vec3<f32>(1.0) - darken) * lit * in.fog, worn.a);
}

fn shaded(texel: vec4<f32>, color: vec4<f32>, tint: u32, appearance: u32) -> vec4<f32> {
    if texel.a <= 0.0 {
        discard;
    }
    if tint == 4u {
        let water = vec3<f32>(0.14901961, 0.44313726, 0.74509805);
        return vec4<f32>(mix(texel.rgb, water, 0.14), texel.a);
    }
    let ramp = tint >= 21u && tint <= 26u;
    let kind = select(tint, tint - 16u, ramp);
    if (kind >= 5u && kind <= 10u) || kind == 12u {
        var shade = 1.0;
        if kind == 7u { shade = 0.72; }
        if kind == 12u { shade = 0.78; }
        shade *= select(1.0, select(0.92, 0.98, kind == 6u), ramp);
        return vec4<f32>(ground(texel.rgb, kind) * shade, texel.a);
    }
    if (appearance & 1u) != 0u && tint <= 3u {
        var scales = vec3<f32>(1000.0);
        switch (appearance >> 1u) & 7u {
            case 0u: { scales = vec3<f32>(990.0, 1000.0, 970.0); }
            case 1u: { scales = vec3<f32>(950.0, 1000.0, 980.0); }
            case 2u: { scales = vec3<f32>(940.0, 1000.0, 930.0); }
            case 3u: { scales = vec3<f32>(1040.0, 980.0, 880.0); }
            case 4u: { scales = vec3<f32>(1030.0, 1000.0, 900.0); }
            default: {}
        }
        let canopy = f32(min((appearance >> 14u) & 1023u, 1000u)) / 1000.0;
        let factor = color.rgb * (scales / 1000.0) * (1.0 - 0.12 * canopy);
        return vec4<f32>(floor(clamp(texel.rgb * 255.0 * factor, vec3<f32>(0.0), vec3<f32>(255.0)) + 0.5) / 255.0, texel.a);
    }
    return texel * color;
}
