// Player planes are aligned at colour page + 3. Unmasked RGB is untouched.
fn owner_texel(original: vec4<f32>, uv: vec2<f32>, page: i32, owner: vec4<f32>) -> vec4<f32> {
    var base = original;
    if owner.a == 0.0 { return base; }
    let mask = floor(textureSampleLevel(sprite_atlas, sprite_sampler, uv, select(page + 1, page + 3, page < 5), 0.0) * 255.0 + 0.5);
    if mask.g == 0.0 { return base; }
    let foreground = mask.g * 255.0;
    let background = mask.b * (255.0 - mask.g);
    let ramp = floor(floor(owner.rgb * 255.0 + 0.5) * (0.65 + min(mask.r, 7.0) / 7.0 * 0.35));
    base = vec4<f32>(floor((ramp * foreground + floor(base.rgb * 255.0 + 0.5) * background) / (foreground + background) + 0.5) / 255.0, base.a);
    return base;
}

fn owner_sample(uv: vec2<f32>, page: i32, owner: vec4<f32>) -> vec4<f32> {
    return owner_texel(textureSampleLevel(sprite_atlas, sprite_sampler, uv, page, 0.0), uv, page, owner);
}

// Object-sprite box average: four quantized nearest taps inside the pixel
// footprint, matching the terrain kernel and clamped to the
// sampled rectangle so atlas neighbours never bleed. Alpha below zero
// signals "no average available; keep the nearest centre".
fn box_average(uv: vec2<f32>, page: i32, rect: vec4<f32>, dx: vec2<f32>, dy: vec2<f32>, owner: vec4<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(sprite_atlas));
    let px = dx * size;
    let py = dy * size;
    let footprint = max(dot(px, px), dot(py, py));
    let bounded = all(abs(rect) <= vec4<f32>(3.402823e38)) && all(abs(uv) <= vec2<f32>(3.402823e38))
        && all(abs(dx) <= vec2<f32>(3.402823e38)) && all(abs(dy) <= vec2<f32>(3.402823e38))
        && footprint > 1.5625 && footprint <= 3.402823e38;
    if !bounded { return vec4<f32>(-1.0); }
    // Mirrored sprites carry a negative uv span, so order the corners.
    let lo = min(rect.xy, rect.xy + rect.zw) + 0.5 / size;
    let hi = max(rect.xy, rect.xy + rect.zw) - 0.5 / size;
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        let sx = select(-1.0, 1.0, (i & 2u) != 0u);
        let sy = select(-1.0, 1.0, (i & 1u) != 0u);
        let tap = floor(owner_sample(clamp(uv + 0.25 * (sx * dx + sy * dy), lo, hi), page, owner) * 255.0 + 0.5);
        sum += vec4<f32>(tap.rgb * tap.a, tap.a);
    }
    if sum.a == 0.0 { return vec4<f32>(-1.0); }
    return vec4<f32>(floor(sum.rgb / sum.a + 0.5) / 255.0, floor(sum.a / 4.0 + 0.5) / 255.0);
}

fn terrain_sample(uv: vec2<f32>, page: u32, rect: vec4<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    var center = textureSampleLevel(sprite_atlas, sprite_sampler, uv, i32(page), 0.0);
    let size = vec2<f32>(textureDimensions(sprite_atlas));
    let extent = rect.zw * size;
    let local = (uv - rect.xy - 0.5 / size) / max(rect.zw - 1.0 / size, 1.0 / size) - 0.5;
    let distance = abs(local.x) + abs(local.y);
    let diamond = abs(extent.x - 2.0 * extent.y + 1.0) <= 1.0;
    if center.a == 0.0 && diamond && min(extent.x, extent.y) > 2.0 && distance >= 0.47 && distance <= 0.50001 {
        let inward = mix(uv, rect.xy + rect.zw * 0.5, 2.0 / min(extent.x, extent.y));
        center = textureSampleLevel(sprite_atlas, sprite_sampler, inward, i32(page), 0.0);
    }
    let px = dx * size;
    let py = dy * size;
    let footprint = max(dot(px, px), dot(py, py));
    if !(all(abs(rect) <= vec4<f32>(3.402823e38)) && all(abs(uv) <= vec2<f32>(3.402823e38)) && all(abs(dx) <= vec2<f32>(3.402823e38)) && all(abs(dy) <= vec2<f32>(3.402823e38)) && footprint > 1.5625 && footprint <= 3.402823e38) { return center; }
    let lo = rect.xy + 0.5 / size;
    let hi = lo + max(rect.zw - 1.0 / size, vec2<f32>(0.0));
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < 4u; i++) {
        let sx = select(-1.0, 1.0, (i & 2u) != 0u);
        let sy = select(-1.0, 1.0, (i & 1u) != 0u);
        let tap = floor(textureSampleLevel(sprite_atlas, sprite_sampler, clamp(uv + 0.25 * (sx * dx + sy * dy), lo, hi), i32(page), 0.0) * 255.0 + 0.5);
        sum += vec4<f32>(tap.rgb * tap.a, tap.a);
    }
    if sum.a == 0.0 { return center; }
    return vec4<f32>(floor(sum.rgb / sum.a + 0.5) / 255.0, center.a);
}


fn tiled_layer(header: vec4<f32>, tile: vec2<i32>, local: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> Layer {
    let columns = u32(header.z);
    let rows = u32(header.w);
    var at_local=local;var at_gx=gx;var at_gy=gy;
    let hash = bitcast<u32>(tile.x) * 7u + bitcast<u32>(tile.y) * 13u;
    var index = select(hash, ~hash + 1u, hash > 2147483648u) % u32(max(header.y, 1.0));
    if (columns & 32768u) != 0u {
        let grid=vec2<u32>(columns & 31u,rows & 31u);
        let period=vec2<u32>((columns >> 5u) & 511u,(rows >> 5u) & 511u);
        let scale=vec2<f32>(grid)/vec2<f32>(period);
        let inside=vec2<f32>(local.x+local.y-0.5,local.y-local.x+0.5);
        let p=(vec2<f32>(vec2<u32>(wrap(tile.x,period.x),wrap(tile.y,period.y)))+inside)*scale;
        let cell=vec2<u32>(floor(p)) % grid;
        index=cell.y*grid.x+cell.x;at_local=fract(p);
        at_gx=vec2<f32>(gx.x+gx.y,gx.y-gx.x)*scale;
        at_gy=vec2<f32>(gy.x+gy.y,gy.y-gy.x)*scale;
    } else if columns > 0u && rows > 0u {
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
    layer.uv = layer.rect.xy + 0.5 / size + at_local * span;
    layer.dx = at_gx * span;
    layer.dy = at_gy * span;
    return layer;
}

