// Coverage effects specialize the shared sprite vertex program. No scene work
// or extra instance upload: final depth/painter coverage decides every contour.
export function coverageShaders(sources) {
    const vertex = sources[0][0].replace('void main()', 'void spriteVertex()');
    const contourVertex = vertex + `
void main() {
    spriteVertex();
    if (color.w < 0.0 || (pages.w & 56u) == 0u) gl_Position = vec4(2,2,2,1);
}`;
    const world = sources.map(([v, f]) => [v, f.replace('out vec4 result;',
        'layout(location=0) out vec4 result;\nlayout(location=1) out highp uint coverage;')
        .replace('void main()', 'void worldColour()') + `
void main() {
    worldColour();
    bool body = vSolid == 0u && result.a >= .8;
    coverage = (body && (vPages.w & 4u) != 0u ? 1u : 0u)
             | (body && (vPages.w & 48u) != 0u ? 2u : 0u);
}`]);
    // Only alpha/discard/depth determine mask bits. Leave RGB unwritten so
    // the shader compiler eliminates terrain colour, tint and owner-ramp work.
    const mask = world.map(([v, f]) => [v,
        f.replace('layout(location=0) out vec4 result;', 'vec4 result;')]);
    return [...world, [contourVertex, CONTOUR], ...mask];
}

const CONTOUR = `#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2DArray atlas;
uniform highp usampler2D mask;
uniform vec2 maskScale;
in vec2 vUv;
flat in uvec4 vPages;
flat in vec4 vRect;
flat in vec4 vRect2;
out vec4 result;
float alphaAt(vec2 uv) {
    vec2 lo = min(vRect.xy, vRect.xy + vRect.zw);
    vec2 hi = max(vRect.xy, vRect.xy + vRect.zw);
    if (any(lessThan(uv, lo)) || any(greaterThanEqual(uv, hi))) return 0.0;
    return textureLod(atlas, vec3(uv, float(vPages.x)), 0.0).a;
}
float edge(vec2 dx, vec2 dy, float width) {
    float x = min(alphaAt(vUv - dx * width), alphaAt(vUv + dx * width));
    float y = min(alphaAt(vUv - dy * width), alphaAt(vUv + dy * width));
    return 1.0 - min(x, y);
}
void main() {
    vec2 dx = dFdx(vUv), dy = dFdy(vUv);
    uint coverage = texelFetch(mask, ivec2(gl_FragCoord.xy * maskScale), 0).r;
    if (alphaAt(vUv) < .8) discard;
    if ((vPages.w & 48u) != 0u) {
        if ((coverage & 2u) == 0u) discard;
        float opacity = edge(dx, dy, 1.25) * .65;
        if ((vPages.w & 32u) != 0u)
            opacity = edge(dx, dy, 1.25) * .4 + edge(dx, dy, 3.0) * .22 + edge(dx, dy, 5.0) * .12;
        if (opacity <= .01) discard;
        result = vec4(1,.88,.55,opacity); return;
    }
    if ((coverage & 1u) == 0u || edge(dx, dy, 1.5) < .2) discard;
    result = vec4(vRect2.rgb, 1);
}`;
