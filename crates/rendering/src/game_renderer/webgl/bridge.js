// Graphics-only WebGL2 adapter. Keep this module self-contained: wasm-bindgen
// packages local modules, not arbitrary transitive shader/JS asset imports.
const SIDE = 2048;
const STRIDE = 112;
const PAGES = 3;
const MAX_BYTES = 64 * 1024 * 1024;
const MAX_PIXELS = 4194304;

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
const vec2 corners[6] = vec2[6](
    vec2(-1,-1), vec2(1,-1), vec2(1,1),
    vec2(-1,-1), vec2(1,1), vec2(-1,1));
vec2 terrainUv(uint mode, uint corner) {
    if (mode == 0u) {
        if (corner == 0u) return vec2(.5,0);
        if (corner == 1u) return vec2(1,.5);
        return vec2(.5,1);
    }
    if (mode == 1u) {
        if (corner == 0u) return vec2(.5,0);
        if (corner == 1u) return vec2(.5,1);
        return vec2(0,.5);
    }
    if (mode == 2u) {
        if (corner == 0u) return vec2(.5,0);
        if (corner == 1u) return vec2(1,.5);
        return vec2(0,.5);
    }
    if (mode == 3u) {
        if (corner == 0u) return vec2(1,.5);
        if (corner == 1u) return vec2(.5,1);
        return vec2(0,.5);
    }
    if (mode == 4u) {
        if (corner == 0u) return vec2(.3,.3);
        if (corner == 1u) return vec2(.7,.3);
        return vec2(.7,.7);
    }
    if (mode == 5u) {
        if (corner == 0u) return vec2(.3,.3);
        if (corner == 1u) return vec2(.7,.7);
        return vec2(.3,.7);
    }
    return vec2(0);
}
vec2 terrainAtlasUv(vec4 rect, vec2 local) {
    vec2 pixel = 1.0 / vec2(textureSize(atlas, 0).xy);
    return rect.xy + pixel * .5 + local * max(rect.zw - pixel, vec2(0));
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
    vUv2 = vec2(0);
    vUv3 = vec2(0);
    vWeights = vec3(1,0,0);
    if (color.w < 0.0) {
        uint corner = min(vertex, 2u);
        vec2 points[3] = vec2[3](positionRadius.xy, positionRadius.zw, color.xy);
        gl_Position = vec4(points[corner], 2.0 * depths[corner] - 1.0, 1);
        if (color.w == -2.0) {
            vColor = vec4(uv.xyz, 1);
            vUv = vec2(0);
            vSolid = 2u;
            vTint = 0u;
        } else {
            uint code = uint(color.z);
            vec2 local = terrainUv(code % 8u, corner);
            vUv = terrainAtlasUv(uv, local);
            vColor = vec4(terrainTint(code / 8u), 1);
            vSolid = 0u;
            vTint = code / 8u;
            if (color.w == -3.0) {
                vUv2 = terrainAtlasUv(terrainBlend0, local);
                vUv3 = terrainAtlasUv(terrainBlend1, local);
                vec3 weights[3] = vec3[3](vec3(1,0,0), vec3(0,1,0), vec3(0,0,1));
                vWeights = weights[corner];
                if ((pages.w & 1u) != 0u) {
                    float floorStrength = float(min((pages.w >> 4u) & 1023u, 1000u)) / 1000.0;
                    vWeights = vec3(1.0 - floorStrength, floorStrength, 0);
                }
                vSolid = 3u;
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
out vec4 result;
void main() {
    if (vSolid == 2u) {
        result = vColor;
        return;
    }
    vec4 texel = textureLod(atlas, vec3(vUv, float(vPages.x)), 0.0);
    if (vSolid == 3u) {
        texel = texel * vWeights.x
              + textureLod(atlas, vec3(vUv2, float(vPages.y)), 0.0) * vWeights.y
              + textureLod(atlas, vec3(vUv3, float(vPages.z)), 0.0) * vWeights.z;
    }
    if (texel.a <= 0.0) discard;
    if (vTint == 4u) {
        result = vec4(mix(texel.rgb, vec3(.14901961,.44313726,.74509805), .14), texel.a);
        return;
    }
    bool ramp = vTint >= 21u && vTint <= 26u;
    uint kind = ramp ? vTint - 16u : vTint;
    if ((kind >= 5u && kind <= 10u) || kind == 12u) {
        float detail = dot(texel.rgb, vec3(1.0 / 3.0));
        vec3 base = vec3(.18);
        float amount = .55;
        float shade = 1.0;
        if (kind == 6u) { base = vec3(.78,.79,.78); amount = .12; }
        if (kind == 7u) { shade = .72; }
        if (kind == 12u) { shade = .78; }
        if (kind == 8u) { base = vec3(.42,.57,.65); amount = .20; }
        if (kind == 9u) { base = vec3(.10,.08,.05); amount = .35; }
        if (kind == 10u) { base = vec3(.22,.36,.33); amount = .25; }
        shade *= ramp ? .92 : 1.0;
        result = vec4((base + detail * amount) * shade, texel.a);
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

export class AoeWebGl {
    constructor(canvas) {
        const gl = canvas.getContext('webgl2', {
            alpha: false, antialias: false, depth: true, stencil: false,
            premultipliedAlpha: false, preserveDrawingBuffer: false,
        });
        if (!gl) throw new Error('WebGL2 unavailable');
        this.gl = gl;
        this.canvas = canvas;
        this.bufferBytes = 0;
        this.atlasReady = false;
        this.disposed = false;
        this.shaders = [];
        this.atlasPixels = null;
        this.restoreError = null;
        this.initialize();
        this.onLost = event => event.preventDefault();
        this.onRestored = () => {
            try {
                this.initialize();
                if (this.atlasPixels) this.upload(this.atlasPixels);
                canvas.dispatchEvent(new Event('aoe-renderer-restored'));
            } catch (error) {
                this.restoreError = error;
            }
        };
        canvas.addEventListener('webglcontextlost', this.onLost);
        canvas.addEventListener('webglcontextrestored', this.onRestored);
    }

    initialize() {
        const gl = this.gl;
        this.bufferBytes = 0;
        this.atlasReady = false;
        this.shaders = [];
        try {
            if (gl.getParameter(gl.MAX_TEXTURE_SIZE) < SIDE ||
                gl.getParameter(gl.MAX_VERTEX_TEXTURE_IMAGE_UNITS) < 1 ||
                gl.getParameter(gl.MAX_VERTEX_ATTRIBS) < 7 ||
                gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS) < PAGES ||
                gl.getParameter(gl.DEPTH_BITS) < 24) {
                throw new Error('WebGL2 sprite/depth limits unavailable');
            }
            this.maxBackingSide = gl.getParameter(gl.MAX_RENDERBUFFER_SIZE);
            this.maxViewport = gl.getParameter(gl.MAX_VIEWPORT_DIMS);
            this.program = gl.createProgram();
            if (!this.program) throw new Error('WebGL2 program allocation failed');
            for (const [type, source] of [[gl.VERTEX_SHADER, VERTEX], [gl.FRAGMENT_SHADER, FRAGMENT]]) {
                const shader = gl.createShader(type);
                if (!shader) throw new Error('WebGL2 shader allocation failed');
                this.shaders.push(shader);
                gl.shaderSource(shader, source);
                gl.compileShader(shader);
                if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
                    throw new Error('WebGL2 shader: ' + gl.getShaderInfoLog(shader));
                }
                gl.attachShader(this.program, shader);
            }
            gl.linkProgram(this.program);
            if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
                throw new Error('WebGL2 link: ' + gl.getProgramInfoLog(this.program));
            }
            for (const shader of this.shaders) {
                gl.detachShader(this.program, shader);
                gl.deleteShader(shader);
            }
            this.shaders = [];
            this.vao = gl.createVertexArray();
            this.buffer = gl.createBuffer();
            this.texture = gl.createTexture();
            if (!this.vao || !this.buffer || !this.texture) {
                throw new Error('WebGL2 resource allocation failed');
            }
            gl.bindVertexArray(this.vao);
            gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
            for (let slot = 0; slot < 6; slot++) {
                gl.enableVertexAttribArray(slot);
                gl.vertexAttribPointer(slot, 4, gl.FLOAT, false, STRIDE, slot * 16);
                gl.vertexAttribDivisor(slot, 1);
            }
            gl.enableVertexAttribArray(6);
            gl.vertexAttribIPointer(6, 4, gl.UNSIGNED_INT, STRIDE, 96);
            gl.vertexAttribDivisor(6, 1);
            gl.activeTexture(gl.TEXTURE0);
            gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
            gl.texImage3D(gl.TEXTURE_2D_ARRAY, 0, gl.RGBA8, SIDE, SIDE, PAGES, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
            gl.useProgram(this.program);
            gl.uniform1i(gl.getUniformLocation(this.program, 'atlas'), 0);
            this.resize(this.canvas.width, this.canvas.height);
            this.check();
        } catch (error) {
            this.dispose();
            throw error;
        }
    }

    check() {
        if (this.disposed || this.gl.isContextLost()) throw new Error('WebGL2 context lost');
        const code = this.gl.getError();
        if (code !== this.gl.NO_ERROR) throw new Error('WebGL2 graphics error ' + code);
    }

    validateSize(width, height) {
        if (!Number.isInteger(width) || !Number.isInteger(height) || width < 0 || height < 0 ||
            width > 4096 || height > 4096 ||
            width > this.maxBackingSide || height > this.maxBackingSide ||
            width > this.maxViewport[0] || height > this.maxViewport[1] ||
            width * height > MAX_PIXELS) {
            throw new Error('WebGL2 backing-store limit exceeded');
        }
    }

    resize(width, height) {
        this.check();
        this.validateSize(width, height);
        if (this.canvas.width !== width) this.canvas.width = width;
        if (this.canvas.height !== height) this.canvas.height = height;
        this.check();
    }

    upload(pixels) {
        this.check();
        if (pixels.byteLength !== SIDE * SIDE * 4 * PAGES) throw new Error('Invalid WebGL2 atlas size');
        const gl = this.gl;
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
        // Typed bytes preserve the WebGPU atlas row convention, without browser
        // image-source flipping, premultiplication, or colour-space conversion.
        gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
        gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
        gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, 0, SIDE, SIDE, PAGES, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        this.check();
        this.atlasReady = true;
        // One bounded 48 MiB source atlas permits device-context restoration.
        this.atlasPixels = pixels;
    }

    render(instances) {
        if (this.restoreError) throw this.restoreError;
        if (!this.disposed && this.gl.isContextLost()) return false;
        this.check();
        if (!this.atlasReady) throw new Error('WebGL2 atlas not uploaded');
        if (instances.byteLength % STRIDE !== 0 || instances.byteLength > MAX_BYTES) {
            throw new Error('WebGL2 instance-buffer limit exceeded');
        }
        const gl = this.gl;
        this.validateSize(this.canvas.width, this.canvas.height);
        if (!this.canvas.width || !this.canvas.height) return false;
        gl.useProgram(this.program);
        gl.bindVertexArray(this.vao);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
        if (instances.byteLength > this.bufferBytes) {
            gl.bufferData(gl.ARRAY_BUFFER, instances.byteLength, gl.DYNAMIC_DRAW);
            this.check();
            this.bufferBytes = instances.byteLength;
        }
        if (instances.byteLength) gl.bufferSubData(gl.ARRAY_BUFFER, 0, instances);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
        gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
        gl.disable(gl.CULL_FACE);
        gl.disable(gl.SCISSOR_TEST);
        gl.disable(gl.DITHER);
        gl.enable(gl.DEPTH_TEST);
        gl.depthFunc(gl.LEQUAL);
        gl.depthMask(true);
        gl.depthRange(0, 1);
        gl.enable(gl.BLEND);
        gl.blendEquation(gl.FUNC_ADD);
        gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
        gl.colorMask(true, true, true, true);
        gl.clearColor(.16, .29, .14, 1);
        gl.clearDepth(1);
        gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
        if (instances.byteLength) gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, instances.byteLength / STRIDE);
        this.check();
        return true;
    }

    dispose() {
        if (this.disposed) return;
        const gl = this.gl;
        for (const shader of this.shaders) gl.deleteShader(shader);
        if (this.program) gl.deleteProgram(this.program);
        if (this.buffer) gl.deleteBuffer(this.buffer);
        if (this.texture) gl.deleteTexture(this.texture);
        if (this.vao) gl.deleteVertexArray(this.vao);
        this.canvas.removeEventListener('webglcontextlost', this.onLost);
        this.canvas.removeEventListener('webglcontextrestored', this.onRestored);
        this.atlasPixels = null;
        this.disposed = true;
        this.bufferBytes = 0;
    }
}
