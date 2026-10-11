// Direct GLSL ES 3.00 translation of sprites.wgsl. Six generated vertices keep
// quads and surfaces in one ordered draw; a surface's second triangle degenerates.
const VERTEX = `#version 300 es
precision highp float;
precision highp int;
layout(location=0) in vec4 positionRadius;
layout(location=1) in vec4 color;
layout(location=2) in vec4 uv;
layout(location=3) in vec4 depths;
layout(location=4) in vec4 terrainBlend0;
layout(location=5) in vec4 terrainBlend1;
layout(location=6) in uvec4 pages;
uniform highp sampler2DArray atlas;
out vec4 vColor;
out vec2 vUv;
out vec2 vUv2;
out vec2 vUv3;
out vec3 vWeights;
flat out uint vSolid;
flat out uint vTint;
flat out uvec4 vPages;
flat out vec4 vRect;
flat out vec4 vRect2;
flat out vec4 vRect3;
const vec2 corners[6] = vec2[6](
    vec2(-1,-1), vec2(1,-1), vec2(1,1),
    vec2(-1,-1), vec2(1,1), vec2(-1,1));
vec2 terrainUv(uint mode, uint corner) {
    const vec2 table[18] = vec2[18](
        vec2(.5,0), vec2(1,.5), vec2(.5,1), vec2(.5,0), vec2(.5,1), vec2(0,.5),
        vec2(.5,0), vec2(1,.5), vec2(0,.5), vec2(1,.5), vec2(.5,1), vec2(0,.5),
        vec2(.3,.3), vec2(.7,.3), vec2(.7,.7), vec2(.3,.3), vec2(.7,.7), vec2(.3,.7));
    return mode < 6u ? table[mode * 3u + corner] : vec2(0);
}
vec2 terrainAtlasUv(vec4 rect, vec2 local) {
    vec2 pixel = 1.0 / vec2(textureSize(atlas, 0).xy);
    return rect.xy + pixel * .5 + local * max(rect.zw - pixel, vec2(0));
}
// Terrain frame directory in rows 0-1 of page 0 (surface_mesh/directory.rs).
vec4 directory(uint entry) {
    return floor(texelFetch(atlas, ivec3(int(entry % 2048u), int(entry / 2048u), 0), 0) * 255.0 + .5);
}
vec4 sheet(uint material) {
    vec4 a = directory(1u + 2u * material), b = directory(2u + 2u * material);
    return vec4(a.x + a.y * 256.0, a.z + a.w * 256.0, b.x + b.y * 256.0, b.z + b.w * 256.0);
}
vec3 terrainTint(uint kind) {
    if (kind == 1u) return vec3(.92);
    if (kind == 2u) return vec3(.78);
    if (kind == 3u) return vec3(.72);
    return vec3(1);
}
void main() {
    uint vertex = uint(gl_VertexID);
    vPages = pages;
    vRect = uv;
    vRect2 = terrainBlend0;
    vRect3 = terrainBlend1;
    vUv2 = vec2(0);
    vUv3 = vec2(0);
    vWeights = vec3(1,0,0);
    if (color.w < 0.0) {
        uint corner = min(vertex, 2u);
        vec2 points[3] = vec2[3](positionRadius.xy, positionRadius.zw, color.xy);
        gl_Position = vec4(points[corner], 2.0 * depths[corner] - 1.0, 1);
        if (color.w == -2.0) {
            vColor = uv;
            vUv = vec2(0);
            vSolid = 2u;
            vTint = 0u;
        } else {
            uint code = uint(color.z);
            vec2 local = terrainUv(code % 8u, corner);
            bool tiled = code % 8u == 6u;
            vUv = terrainAtlasUv(uv, local);
            vColor = vec4(terrainTint(code / 8u), 1);
            vSolid = 1u;
            vTint = code / 8u;
            if (tiled) {
                vec2 locals[3] = vec2[3](uv.xy, uv.zw, terrainBlend0.xy);
                vUv = locals[corner];
                vRect = sheet(pages.z & 255u);
                vSolid = 5u;
            }
            if (color.w == -3.0) {
                if (tiled) {
                    vRect2 = sheet((pages.z >> 8u) & 255u);
                    if (uint(terrainBlend0.z) != 3u) vRect3 = sheet((pages.z >> 16u) & 255u);
                } else {
                    vUv2 = terrainAtlasUv(terrainBlend0, local);
                    vUv3 = terrainAtlasUv(terrainBlend1, local);
                }
                vec3 weights[3] = vec3[3](vec3(1,0,0), vec3(0,1,0), vec3(0,0,1));
                vWeights = weights[corner];
                if ((pages.w & 1u) != 0u) {
                    float floorStrength = float(min((pages.w >> 4u) & 1023u, 1000u)) / 1000.0;
                    if ((pages.w & 536870912u) != 0u) {
                        floorStrength = float((uint(depths.w) >> (corner * 8u)) & 255u) / 255.0;
                    }
                    vWeights = vec3(1.0 - floorStrength, floorStrength, 0);
                    uint mode = uint(terrainBlend0.z);
                    if (tiled && mode != 0u) {
                        uint share = uint(terrainBlend1[corner]);
                        vWeights = vec3(float(share & 255u) / 255.0, float(share >> 8u) / 255.0, floorStrength);
                        if (mode == 3u) {
                            vRect = vec4(terrainBlend0.w, terrainBlend1.w, float((pages.z >> 16u) & 255u), 0);
                            vWeights = vec3(weights[corner].xy, floorStrength);
                        }
                        vSolid |= 8u * mode;
                    }
                }
                vSolid |= 2u;
            }
        }
    } else {
        vec2 corner = corners[vertex];
        gl_Position = vec4(positionRadius.xy + corner * positionRadius.zw,
                           2.0 * depths.x - 1.0, 1);
        vUv = uv.xy + vec2((corner.x + 1.0) * .5, (1.0 - corner.y) * .5) * uv.zw;
        vColor = color;
        vSolid = 0u;
        vTint = 0u;
    }
}`;

const FRAGMENT = `#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2DArray atlas;
in vec4 vColor;
in vec2 vUv;
in vec2 vUv2;
in vec2 vUv3;
in vec3 vWeights;
flat in uint vSolid;
flat in uint vTint;
flat in uvec4 vPages;
flat in vec4 vRect;
flat in vec4 vRect2;
flat in vec4 vRect3;
out vec4 result;
vec4 terrainSample(vec2 uv, uint page, vec4 rect, vec2 dx, vec2 dy) {
    vec4 center = textureLod(atlas, vec3(uv, float(page)), 0.0);
    vec2 size = vec2(textureSize(atlas, 0).xy);
    vec2 px = dx * size, py = dy * size;
    float footprint = max(dot(px,px), dot(py,py));
    if (!(all(lessThanEqual(abs(rect),vec4(3.402823e38))) && all(lessThanEqual(abs(uv),vec2(3.402823e38))) && all(lessThanEqual(abs(dx),vec2(3.402823e38))) && all(lessThanEqual(abs(dy),vec2(3.402823e38))) && footprint > 1.5625 && footprint <= 3.402823e38)) return center;
    vec2 lo = rect.xy + .5 / size;
    vec2 hi = lo + max(rect.zw - 1.0 / size, vec2(0));
    vec2 offsets[4] = vec2[4](-dx-dy, -dx+dy, dx-dy, dx+dy);
    vec4 sum = vec4(0);
    for (int i=0; i<4; i++) {
        vec4 tap = floor(textureLod(atlas, vec3(clamp(uv + .25 * offsets[i], lo, hi), float(page)), 0.0) * 255.0 + .5);
        sum += vec4(tap.rgb * tap.a, tap.a);
    }
    if (sum.a == 0.0) return center;
    return vec4(floor(sum.rgb / sum.a + .5) / 255.0, center.a);
}
struct Layer { vec2 uv; uint page; vec4 rect; vec2 dx; vec2 dy; };
vec4 directory(uint entry) {
    return floor(texelFetch(atlas, ivec3(int(entry % 2048u), int(entry / 2048u), 0), 0) * 255.0 + .5);
}
vec4 sheet(uint material) {
    vec4 a = directory(1u + 2u * material), b = directory(2u + 2u * material);
    return vec4(a.x + a.y * 256.0, a.z + a.w * 256.0, b.x + b.y * 256.0, b.z + b.w * 256.0);
}

// Euclidean remainder over the whole signed range, in defined unsigned arithmetic.
uint wrap(int value, uint period) {
    return ((uint(value) ^ 2147483648u) % period + period - 2147483648u % period) % period;
}
Layer tiledLayer(vec4 header, ivec2 tile, vec2 local, vec2 gx, vec2 gy) {
    uint columns = uint(header.z), rows = uint(header.w);
    uint hash = uint(tile.x) * 7u + uint(tile.y) * 13u;
    uint index = (hash > 2147483648u ? ~hash + 1u : hash) % uint(max(header.y, 1.0));
    if (columns > 0u && rows > 0u) {
        index = wrap(tile.x, columns) * rows + (rows - wrap(tile.y, rows)) % rows;
    }
    uint entry = uint(header.x) + 2u * index;
    vec4 a = directory(entry), b = directory(entry + 1u);
    uint high = uint(b.z);
    vec2 size = vec2(textureSize(atlas, 0).xy);
    vec4 rect = vec4(a.x + a.y * 256.0, a.z + a.w * 256.0, b.x + float(high & 15u) * 256.0, b.y + float(high >> 4u) * 256.0) / size.xyxy;
    vec2 span = max(rect.zw - 1.0 / size, vec2(0));
    return Layer(rect.xy + .5 / size + local * span, uint(b.w), rect, gx * span, gy * span);
}
// Mixed ground, as sprites.wgsl: procedural layer kernel and world-anchored noise.
vec3 ground(vec3 rgb, uint kind) {
    vec3 base = vec3(.18);
    float amount = .55;
    if (kind == 6u) { base = vec3(.78,.79,.78); amount = .12; }
    else if (kind == 8u) { base = vec3(.42,.57,.65); amount = .20; }
    else if (kind == 9u) { base = vec3(.10,.08,.05); amount = .35; }
    else if (kind == 10u) { base = vec3(.22,.36,.33); amount = .25; }
    else if (kind != 5u && kind != 7u && kind != 12u) return rgb;
    return base + dot(rgb, vec3(1.0 / 3.0)) * amount;
}
vec4 mixed(vec4 texel, uint material) {
    const uint kinds[16] = uint[16](0u,0u,0u,0u,5u,0u,0u,6u,8u,9u,0u,0u,0u,0u,0u,0u);
    return vec4(ground(texel.rgb, kinds[material & 15u]), texel.a);
}
float lattice(ivec2 p) {
    uint hash = (uint(p.x) * 2654435761u) ^ (uint(p.y) * 2246822519u);
    hash = (hash ^ (hash >> 15u)) * 739982445u;
    return float((hash ^ (hash >> 12u)) & 255u) / 255.0;
}
float value(ivec2 tile, vec2 inside, int cells) {
    vec2 scaled = inside * float(cells), cell = floor(scaled), t = scaled - cell;
    vec2 u = t * t * (3.0 - 2.0 * t);
    ivec2 at = tile * cells + ivec2(cell);
    float top = lattice(at) + (lattice(at + ivec2(1,0)) - lattice(at)) * u.x;
    float bottom = lattice(at + ivec2(0,1)) + (lattice(at + ivec2(1,1)) - lattice(at + ivec2(0,1))) * u.x;
    return top + (bottom - top) * u.y;
}
float edge(float share, float noise) {
    return clamp((share - .5) * 4.0 + .5 + (noise - .5) * 2.6, 0.0, 1.0);
}
float junctionShare(uint word, uint material) {
    uint present = 0u, matching = 0u;
    for (uint slot = 0u; slot < 4u; slot++) {
        uint cell = (word >> (slot * 4u)) & 15u;
        present += cell != 15u ? 1u : 0u;
        matching += cell == material ? 1u : 0u;
    }
    return float((matching * 255u + present / 2u) / max(present, 1u)) / 255.0;
}
void main() {
    Layer first = Layer(vUv, vPages.x, vRect, dFdx(vUv), dFdy(vUv));
    Layer second = Layer(vUv2, vPages.y, vRect2, dFdx(vUv2), dFdy(vUv2));
    Layer third = Layer(vUv3, vPages.z, vRect3, dFdx(vUv3), dFdy(vUv3));
    if (vSolid == 2u) {
        result = vColor;
        return;
    }
    bool blend = (vSolid & 3u) == 3u;
    bool landscape = (vPages.w & 1u) != 0u, mixing = vSolid >= 8u, junction = vSolid >= 24u;
    vec2 cell = floor(vUv), inside = vUv - cell;
    ivec2 tile = ivec2(int(vPages.x), int(vPages.y)) + ivec2(cell);
    if ((vSolid & 4u) != 0u && !junction) {
        if (vRect.y < 1.0) discard;
        vec2 local = vec2(.5 + .5 * (inside.x - inside.y), .5 * (inside.x + inside.y));
        vec2 gx = vec2(.5 * (first.dx.x - first.dx.y), .5 * (first.dx.x + first.dx.y));
        vec2 gy = vec2(.5 * (first.dy.x - first.dy.y), .5 * (first.dy.x + first.dy.y));
        first = tiledLayer(vRect, tile, local, gx, gy);
        if (blend) {
            second = tiledLayer(vRect2, tile, local, gx, gy);
            if (!landscape || mixing) third = tiledLayer(vRect3, tile, local, gx, gy);
        }
    }
    vec4 texel;
    if (junction) texel = vec4(0);
    else if (vSolid == 0u || !landscape) texel = textureLod(atlas, vec3(first.uv, float(first.page)), 0.0);
    else texel = terrainSample(first.uv, first.page, first.rect, first.dx, first.dy);
    if (junction) {
        float noise = .6 * value(tile, inside, 1) + .4 * value(tile, inside, 4);
        vec3 bary = vec3(vWeights.xy, 1.0 - vWeights.x - vWeights.y);
        vec2 local = vec2(.5 + .5 * (inside.x - inside.y), .5 * (inside.x + inside.y));
        vec2 gx = vec2(.5 * (first.dx.x - first.dx.y), .5 * (first.dx.x + first.dx.y));
        vec2 gy = vec2(.5 * (first.dy.x - first.dy.y), .5 * (first.dy.x + first.dy.y));
        float cumulative = 0.0, previous = 0.0;
        // A varying bound avoids twelve-way filtered-sample unrolling.
        for (uint index = min(uint(vRect.z), 12u); index > 0u; index--) {
            uint slot = index - 1u, word = uint(vRect[slot / 6u]);
            uint material = (word >> ((slot % 6u) * 4u)) & 15u;
            if (material == 15u) continue;
            vec3 shares = vec3(junctionShare(uint(vRect3.x), material), junctionShare(uint(vRect3.y), material), junctionShare(uint(vRect3.z), material));
            cumulative += dot(shares, bary);
            float threshold = edge(cumulative, noise), weight = (threshold - previous) * (1.0 - vWeights.z);
            previous = threshold;
            if (weight <= 0.0) continue;
            Layer layer = tiledLayer(sheet(material), tile, local, gx, gy);
            texel += mixed(terrainSample(layer.uv, layer.page, layer.rect, layer.dx, layer.dy), material) * weight;
        }
        if (vWeights.z > 0.0) {
            Layer bed = tiledLayer(vRect2, tile, local, gx, gy);
            texel += terrainSample(bed.uv, bed.page, bed.rect, bed.dx, bed.dy) * vWeights.z;
        }
    } else if (mixing) {
        float noise = .6 * value(tile, inside, 1) + .4 * value(tile, inside, 4);
        float over = edge(vWeights.x, noise), middle = edge(vWeights.x + vWeights.y, noise);
        vec3 weights = vSolid >= 16u ? vec3(1.0 - middle, middle - over, over)
                                     : vec3((1.0 - vWeights.z) * (1.0 - over), vWeights.z, (1.0 - vWeights.z) * over);
        texel = mixed(texel, vPages.z) * weights.x
              + mixed(terrainSample(second.uv, second.page, second.rect, second.dx, second.dy), vPages.z >> 8u) * weights.y
              + mixed(terrainSample(third.uv, third.page, third.rect, third.dx, third.dy), vPages.z >> 16u) * weights.z;
    } else if (blend) {
        if (landscape) {
            texel = texel * vWeights.x
                  + terrainSample(second.uv, second.page, second.rect, second.dx, second.dy) * vWeights.y
                  + texel * vWeights.z;
        } else {
            texel = texel * vWeights.x
                  + textureLod(atlas, vec3(second.uv, float(second.page)), 0.0) * vWeights.y
                  + textureLod(atlas, vec3(third.uv, float(third.page)), 0.0) * vWeights.z;
        }
    }
    if (texel.a <= 0.0) discard;
    if (vTint == 4u) {
        result = vec4(mix(texel.rgb, vec3(.14901961,.44313726,.74509805), .14), texel.a);
        return;
    }
    bool ramp = vTint >= 21u && vTint <= 26u;
    uint kind = ramp ? vTint - 16u : vTint;
    if ((kind >= 5u && kind <= 10u) || kind == 12u) {
        float shade = (kind == 7u ? .72 : kind == 12u ? .78 : 1.0) * (ramp ? .92 : 1.0);
        result = vec4(ground(texel.rgb, kind) * shade, texel.a);
        return;
    }
    if ((vPages.w & 1u) != 0u && vTint <= 3u) {
        uint palette = (vPages.w >> 1u) & 7u;
        vec3 scales = vec3(1000);
        if (palette == 0u) scales = vec3(990,1000,970);
        if (palette == 1u) scales = vec3(950,1000,980);
        if (palette == 2u) scales = vec3(940,1000,930);
        if (palette == 3u) scales = vec3(1040,980,880);
        if (palette == 4u) scales = vec3(1030,1000,900);
        float canopy = float(min((vPages.w >> 14u) & 1023u, 1000u)) / 1000.0;
        vec3 factor = vColor.rgb * (scales / 1000.0) * (1.0 - .12 * canopy);
        result = vec4(floor(clamp(texel.rgb * 255.0 * factor, vec3(0), vec3(255)) + .5) / 255.0, texel.a);
        return;
    }
    result = texel * vColor;
}`;

// Registered directly with wasm-bindgen so both local modules are packaged.
export function terrainShaders() { return [VERTEX, FRAGMENT]; }
