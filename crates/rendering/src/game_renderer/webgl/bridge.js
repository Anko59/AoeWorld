// Graphics-only WebGL2 adapter. Rust passes the separately registered shader
// module directly: no unregistered transitive shader/JS asset imports.
const SIDE = 2048;
const STRIDE = 112;
const PAGES = 3;
const MAX_BYTES = 64 * 1024 * 1024;
const MAX_PIXELS = 4194304;

export class AoeWebGl {
    constructor(canvas, sources) {
        this.shaderSources = sources;
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
            for (const [type, source] of [[gl.VERTEX_SHADER, this.shaderSources[0]], [gl.FRAGMENT_SHADER, this.shaderSources[1]]]) {
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
            gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, SIDE, SIDE, PAGES);
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
        // Layer-sized uploads bound browser transfer staging to one page.
        for (let layer = 0; layer < PAGES; layer++) {
            gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, layer, SIDE, SIDE, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels, layer * SIDE * SIDE * 4);
        }
        this.check();
        this.atlasReady = true;
        // The only retained atlas copy (48 MiB) permits device-context
        // restoration; the WASM source vector is released after upload.
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
