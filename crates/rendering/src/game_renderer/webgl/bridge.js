// Graphics-only WebGL2 adapter. Rust passes the separately registered shader
// module directly: no unregistered transitive shader/JS asset imports.
const SIDE = 2048;
const STRIDE = 112;
const MAX_PAGES = 64;
const MAX_BYTES = 64 * 1024 * 1024;
const MAX_PIXELS = 4194304;
const COVERAGE_IDLE_FRAMES = 60;

export class AoeWebGl {
    constructor(canvas, sources) {
        this.shaderSources = sources;
        const gl = canvas.getContext('webgl2', {
            alpha: false, antialias: false, depth: true, stencil: false,
            // The client skips unchanged frames. Retain their pixels after
            // compositing so idle scenes and late captures keep the last draw.
            premultipliedAlpha: false, preserveDrawingBuffer: true,
        });
        if (!gl) throw new Error('WebGL2 unavailable');
        this.gl = gl;
        this.canvas = canvas;
        this.worldBytes = 0;
        this.atlasReady = false;
        this.atlasPages = 8;
        this.disposed = false;
        this.shaders = [];
        this.programs = [];
        this.atlasPixels = null;
        this.atlasMemory = null;
        this.restoreError = null;
        // Telemetry: a lost context that never restores shows as lost > restored.
        this.contextLostCount = 0;
        this.contextRestoredCount = 0;
        this.initialize();
        const readPixels = gl.readPixels;
        const readback = (...args) => {
            readPixels.apply(gl, args);
            // Client-memory readback can retire draws before fences are visible.
            // Finish covers reads that GL rejected before transferring data.
            if (ArrayBuffer.isView(args[6]) && this.pendingFrames.length) {
                gl.finish();
                for (const sync of this.pendingFrames) gl.deleteSync(sync);
                this.pendingFrames.length = 0;
                this.needsFrameValidation = true;
            }
        };
        gl.readPixels = readback;
        this.releaseReadback = () => {
            if (gl.readPixels === readback) gl.readPixels = readPixels;
        };
        this.onLost = event => { this.contextLostCount++; event.preventDefault(); };
        this.onRestored = () => {
            try {
                this.initialize();
                if (this.atlasPixels) {
                    const pixels = this.atlasPixels.buffer === this.atlasMemory.buffer
                        ? this.atlasPixels
                        : new Uint8Array(this.atlasMemory.buffer, this.atlasOffset, SIDE * SIDE * this.atlasPages * 4);
                    this.upload(pixels, this.atlasMemory);
                }
                this.contextRestoredCount++;
                canvas.dispatchEvent(new Event('aoe-renderer-restored'));
            } catch (error) {
                this.restoreError = error;
            }
        };
        canvas.addEventListener('webglcontextlost', this.onLost);
        canvas.addEventListener('webglcontextrestored', this.onRestored);
        this.onPageHide = event => { if (!event.persisted) this.dispose(); };
        window.addEventListener('pagehide', this.onPageHide);
    }
    initialize() {
        const gl = this.gl;
        this.pendingFrames = [];
        this.frameError = null;
        this.waitingForStorage = false;
        this.patching = this.patchesPending = this.drainingForAtlas = this.needsFrameValidation = false;
        this.worldBytes = 0;
        this.atlasReady = false;
        this.shaders = [];
        this.programs = [];
        this.contourBuffer = null;
        this.contourVao = null;
        this.contourBytes = 0;
        try {
            if (gl.getParameter(gl.MAX_TEXTURE_SIZE) < SIDE ||
                gl.getParameter(gl.MAX_VERTEX_TEXTURE_IMAGE_UNITS) < 1 ||
                gl.getParameter(gl.MAX_VERTEX_ATTRIBS) < 7 ||
                gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS) < this.atlasPages ||
                gl.getParameter(gl.DEPTH_BITS) < 24) {
                throw new Error('WebGL2 sprite/depth limits unavailable');
            }
            this.maxBackingSide = Math.min(gl.getParameter(gl.MAX_RENDERBUFFER_SIZE), gl.getParameter(gl.MAX_TEXTURE_SIZE));
            this.maxViewport = gl.getParameter(gl.MAX_VIEWPORT_DIMS);
            for (const sources of this.shaderSources) {
                const program = gl.createProgram();
                if (!program) throw new Error('WebGL2 program allocation failed');
                this.programs.push(program);
                for (const [type, source] of [[gl.VERTEX_SHADER, sources[0]], [gl.FRAGMENT_SHADER, sources[1]]]) {
                    const shader = gl.createShader(type);
                    if (!shader) throw new Error('WebGL2 shader allocation failed');
                    this.shaders.push(shader);
                    gl.shaderSource(shader, source);
                    gl.compileShader(shader);
                    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
                        throw new Error('WebGL2 shader: ' + gl.getShaderInfoLog(shader));
                    }
                    gl.attachShader(program, shader);
                }
                gl.linkProgram(program);
                if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
                    throw new Error('WebGL2 link: ' + gl.getProgramInfoLog(program));
                }
                for (const shader of this.shaders) {
                    gl.detachShader(program, shader);
                    gl.deleteShader(shader);
                }
                this.shaders = [];
                gl.useProgram(program);
                gl.uniform1i(gl.getUniformLocation(program, 'atlas'), 0);
                gl.uniform1i(gl.getUniformLocation(program, 'mask'), 1);
            }
            this.maskScale = gl.getUniformLocation(this.programs[2], 'maskScale');
            this.buffer = gl.createBuffer();
            this.texture = gl.createTexture();
            if (!this.buffer || !this.texture) throw new Error('WebGL2 resource allocation failed');
            this.vao = this.instanceLayout(this.buffer);
            gl.activeTexture(gl.TEXTURE0);
            gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
            gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
            gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, SIDE, SIDE, this.atlasPages);
            this.coverage = null;
            this.mask = null;
            this.depth = null;
            this.coverageSize = [0, 0];
            this.coverageIdleFrames = 0;
            this.resize(this.canvas.width, this.canvas.height);
            this.check();
        } catch (error) {
            this.dispose();
            throw error;
        }
    }
    instanceLayout(buffer) {
        const gl = this.gl, vao = gl.createVertexArray();
        if (!vao) throw new Error('WebGL2 instance layout allocation failed');
        gl.bindVertexArray(vao);
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        for (let slot = 0; slot < 6; slot++) {
            gl.enableVertexAttribArray(slot);
            gl.vertexAttribPointer(slot, 4, gl.FLOAT, false, STRIDE, slot * 16);
            gl.vertexAttribDivisor(slot, 1);
        }
        gl.enableVertexAttribArray(6);
        gl.vertexAttribIPointer(6, 4, gl.UNSIGNED_INT, STRIDE, 96);
        gl.vertexAttribDivisor(6, 1);
        return vao;
    }
    createCoverage() {
        const gl = this.gl;
        this.coverage = gl.createFramebuffer();
        this.mask = gl.createTexture();
        this.depth = gl.createRenderbuffer();
        if (!this.coverage || !this.mask || !this.depth)
            throw new Error('WebGL2 coverage allocation failed');
        this.contourBuffer = gl.createBuffer();
        if (!this.contourBuffer) throw new Error('WebGL2 contour buffer allocation failed');
        this.contourVao = this.instanceLayout(this.contourBuffer);
    }
    releaseCoverage() {
        const gl = this.gl;
        if (this.coverage) gl.deleteFramebuffer(this.coverage);
        if (this.mask) gl.deleteTexture(this.mask);
        if (this.depth) gl.deleteRenderbuffer(this.depth);
        if (this.contourVao) gl.deleteVertexArray(this.contourVao);
        if (this.contourBuffer) gl.deleteBuffer(this.contourBuffer);
        this.contourVao = this.contourBuffer = null;
        this.contourBytes = 0;
        this.coverage = this.mask = this.depth = null;
        this.coverageSize = [0, 0];
        this.coverageIdleFrames = 0;
    }
    resizeCoverage(width, height) {
        if (!width || !height) {
            this.releaseCoverage();
            return;
        }
        // Only coverage is downsampled. World colour and the contour's alpha
        // kernel still render at the canvas resolution, without a colour copy.
        const scale = width * height > 1048576 ? 2 : 1;
        width = Math.ceil(width / scale);
        height = Math.ceil(height / scale);
        const [previousWidth, previousHeight] = this.coverageSize;
        if (previousWidth === width && previousHeight === height) return;
        if (!this.coverage) this.createCoverage();
        const gl = this.gl;
        // Replace storage in the same objects; never retain old backing sizes.
        gl.bindTexture(gl.TEXTURE_2D, this.mask);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8UI, width, height, 0, gl.RED_INTEGER, gl.UNSIGNED_BYTE, null);
        gl.bindRenderbuffer(gl.RENDERBUFFER, this.depth);
        gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT24, width, height);
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.coverage);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT1, gl.TEXTURE_2D, this.mask, 0);
        gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, this.depth);
        gl.drawBuffers([gl.NONE, gl.COLOR_ATTACHMENT1]);
        if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE)
            throw new Error('WebGL2 coverage framebuffer incomplete');
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        this.coverageSize = [width, height];
    }
    get bufferBytes() { return this.worldBytes + this.contourBytes; }
    get textureBytes() {
        // R8UI + DEPTH_COMPONENT24 (four-byte storage), shared by both effects.
        // Excludes the browser framebuffer and driver overhead.
        return this.disposed ? 0 : SIDE * SIDE * this.atlasPages * 4 +
            this.coverageSize[0] * this.coverageSize[1] * 5;
    }
    check() {
        if (this.disposed || this.gl.isContextLost()) throw new Error('WebGL2 context lost');
        const code = this.gl.getError();
        if (code !== this.gl.NO_ERROR) throw new Error('WebGL2 graphics error ' + code);
    }
    // Latch errors belonging to unfinished frames for health polling.
    latchSubmittedError() {
        const code = this.pendingFrames.length ? this.gl.getError() : 0;
        if (code && !this.frameError) this.frameError = new Error('WebGL2 graphics error ' + code);
    }

    poll() {
        if (this.frameError) throw this.frameError;
        if (this.restoreError) throw this.restoreError;
        if (this.disposed) throw new Error('WebGL2 context lost');
        const gl = this.gl;
        if (gl.isContextLost()) return false;
        try {
            while (this.pendingFrames.length) {
                const sync = this.pendingFrames[0], status = gl.clientWaitSync(sync, 0, 0);
                if (status === gl.TIMEOUT_EXPIRED) break;
                if (status === gl.WAIT_FAILED) throw new Error('WebGL2 frame completion failed');
                gl.deleteSync(sync); this.pendingFrames.shift(); this.needsFrameValidation = true;
            }
            // getError can wait for newer drawing even after the oldest fence signals.
            if (this.atlasUpdatesReady) {
                this.waitingForStorage = false;
                if (this.needsFrameValidation) {
                    this.check(); this.needsFrameValidation = false;
                }
            }
        } catch (error) { this.frameError = error; throw error; }
        return true;
    }
    get atlasUpdatesReady() { return this.pendingFrames.length === 0; }
    get frameBacklogged() {
        return this.pendingFrames.length >= 2 ||
            (!this.atlasUpdatesReady &&
             (this.drainingForAtlas || this.waitingForStorage || this.needsFrameValidation));
    }
    prioritizeAtlasUpdates() { this.drainingForAtlas = !this.atlasUpdatesReady; }
    beginPatches() {
        if (this.patching) throw new Error('WebGL2 atlas batch already active');
        if (this.disposed) throw new Error('WebGL2 context lost');
        if (this.atlasUpdatesReady) this.drainingForAtlas = false;
        this.latchSubmittedError(); this.patching = true; this.patchesPending = false;
    }
    finishPatches() {
        const pending = this.patchesPending;
        this.patching = this.patchesPending = false;
        try { if (pending) this.check(); }
        catch (error) { if (this.disposed || !this.gl.isContextLost()) throw error; }
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
        if (this.coverage) this.resizeCoverage(width, height);
        this.check();
    }

    upload(pixels, memory) {
        const pages = pixels.byteLength / (SIDE * SIDE * 4);
        if (!Number.isInteger(pages) || pages < 8 || pages > MAX_PAGES || pages % 2)
            throw new Error('Invalid WebGL2 atlas size');
        try {
            this.check();
            if (pages !== this.atlasPages) {
                const gl = this.gl;
                if (gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS) < pages)
                    throw new Error('WebGL2 atlas layer budget unavailable');
                gl.deleteTexture(this.texture);
                this.texture = gl.createTexture();
                if (!this.texture) throw new Error('WebGL2 atlas allocation failed');
                gl.activeTexture(gl.TEXTURE0);
                gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
                gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
                gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
                gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
                gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
                gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, SIDE, SIDE, pages);
                this.atlasPages = pages;
            }
            const gl = this.gl;
            gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
            // Typed bytes preserve the WebGPU atlas row convention, without browser
            // image-source flipping, premultiplication, or colour-space conversion.
            gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
            gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
            gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
            // A page-sized call alone does not bound driver staging: all eight
            // transfers can remain queued together. Complete each startup/restore
            // page before lending the next, so at most one 16 MiB transfer is live.
            for (let layer = 0; layer < this.atlasPages; layer++) {
                gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, layer, SIDE, SIDE, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels, layer * SIDE * SIDE * 4);
                gl.finish();
            }
            this.check();
            this.atlasReady = true;
        } catch (error) {
            if (this.disposed || !this.gl.isContextLost()) throw error;
            this.atlasPages = pages;
        }
        // Retain the complete Rust-owned source for context restoration.
        this.atlasPixels = pixels;
        this.atlasMemory = memory;
        this.atlasOffset = pixels.byteOffset;
    }

    patch(page, x, y, width, height, pixels) {
        if ((!this.atlasReady && !this.gl.isContextLost()) || ![page,x,y,width,height].every(Number.isInteger) ||
            page < 0 || page >= this.atlasPages || x < 0 || y < 0 || width <= 0 || height <= 0 ||
            x + width > SIDE || y + height > SIDE || pixels.length !== width * height * 4)
            throw new Error('Invalid WebGL2 atlas patch');
        const gl = this.gl;
        try {
            if (this.disposed) throw new Error('WebGL2 context lost');
            if (!this.patching) this.latchSubmittedError(); gl.activeTexture(gl.TEXTURE0);
            gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.texture);
            gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
            gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
            gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
            gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, x, y, page, width, height, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
            if (this.patching) this.patchesPending = true;
            else this.check();
        } catch (error) {
            // Rust already copied this plane into the restoration source.
            // Complete the other plane there, then restore the whole atlas.
            if (this.disposed || !gl.isContextLost()) throw error;
        }
    }
    render(instances, contours, black, effects, start, end) {
        if (!this.poll() || this.frameBacklogged) return false;
        if (this.drainingForAtlas) {
            this.prioritizeAtlasUpdates();
            return false;
        }
        if (!this.atlasReady) throw new Error('WebGL2 atlas not uploaded');
        if (instances.byteLength % STRIDE !== 0 || instances.byteLength > MAX_BYTES) {
            throw new Error('WebGL2 instance-buffer limit exceeded');
        }
        if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || end < start || end > instances.byteLength || start % STRIDE || end % STRIDE) throw new Error('WebGL2 upload range invalid');
        const gl = this.gl;
        this.validateSize(this.canvas.width, this.canvas.height);
        if (!this.canvas.width || !this.canvas.height) {
            this.releaseCoverage();
            return false;
        }
        const count = instances.byteLength / STRIDE;
        // Allocation validation is synchronous. Keep existing storage while
        // an earlier frame is in flight, then retry the latest complete view.
        if (!this.atlasUpdatesReady &&
            (instances.byteLength > this.worldBytes ||
             (effects && (!this.coverage || contours.byteLength > this.contourBytes)))) {
            this.waitingForStorage = true;
            return false;
        }
        // Shared Rust normalization selects hover, or units AND occluders.
        // An unoccluded unit needs no coverage backing or second pass.
        if (effects) {
            this.coverageIdleFrames = 0;
            this.resizeCoverage(this.canvas.width, this.canvas.height);
        } else if (this.coverage && ++this.coverageIdleFrames >= COVERAGE_IDLE_FRAMES) {
            // Camera/hover changes can briefly remove all effects. Keep one
            // size-bounded backing through those gaps instead of reallocating
            // and synchronously validating a framebuffer on the next frame.
            this.releaseCoverage();
        }
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        gl.bindVertexArray(this.vao);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
        if (instances.byteLength > this.worldBytes) {
            gl.bufferData(gl.ARRAY_BUFFER, instances.byteLength, gl.DYNAMIC_DRAW);
            this.check();
            this.worldBytes = instances.byteLength;
            start = 0; end = instances.byteLength; // Growth/restoration invalidates every byte.
        }
        if (end > start) gl.bufferSubData(gl.ARRAY_BUFFER, start, instances.subarray(start, end));
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
        const clear = black ? [0, 0, 0, 1] : [.16, .29, .14, 1];
        gl.clearDepth(1);
        gl.clearColor(...clear);
        gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
        // Branch on the existing packet tags in the shader. One ordered
        // instanced draw keeps interleaved shoreline and actor pixels intact.
        if (!count) {
            this.needsFrameValidation = true;
            return true;
        }
        gl.useProgram(this.programs[0]);
        gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, count);
        if (effects) {
            // Coverage/depth only: the default framebuffer already owns the
            // full-resolution world colour and depth used by the contour pass.
            gl.bindFramebuffer(gl.FRAMEBUFFER, this.coverage);
            gl.viewport(0, 0, ...this.coverageSize);
            gl.clearBufferuiv(gl.COLOR, 1, [0, 0, 0, 0]);
            gl.clear(gl.DEPTH_BUFFER_BIT);
            gl.useProgram(this.programs[1]);
            gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, count);
            gl.bindFramebuffer(gl.FRAMEBUFFER, null);
            gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
            gl.activeTexture(gl.TEXTURE1);
            gl.bindTexture(gl.TEXTURE_2D, this.mask);
            gl.depthFunc(gl.GEQUAL);
            gl.depthMask(false);
            gl.useProgram(this.programs[2]);
            gl.uniform2f(this.maskScale,
                this.coverageSize[0] / this.canvas.width, this.coverageSize[1] / this.canvas.height);
            gl.bindVertexArray(this.contourVao);
            gl.bindBuffer(gl.ARRAY_BUFFER, this.contourBuffer);
            if (contours.byteLength > this.contourBytes) {
                this.contourBytes = Math.max(contours.byteLength, Math.min(MAX_BYTES, this.contourBytes * 2));
                gl.bufferData(gl.ARRAY_BUFFER, this.contourBytes, gl.DYNAMIC_DRAW);
            }
            gl.bufferSubData(gl.ARRAY_BUFFER, 0, contours);
            gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, contours.byteLength / STRIDE);
            gl.bindTexture(gl.TEXTURE_2D, null);
            gl.activeTexture(gl.TEXTURE0);
        }
        const sync = gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE, 0);
        if (!sync) throw new Error('WebGL2 frame completion unavailable');
        this.pendingFrames.push(sync);
        gl.flush();
        // poll() validates completed submissions without flushing this one.
        return true;
    }

    dispose() {
        if (this.disposed) return;
        const gl = this.gl;
        this.releaseReadback?.();
        window.removeEventListener('pagehide', this.onPageHide);
        for (const sync of this.pendingFrames) gl.deleteSync(sync);
        this.pendingFrames = [];
        for (const shader of this.shaders) gl.deleteShader(shader);
        for (const program of this.programs) gl.deleteProgram(program);
        if (this.buffer) gl.deleteBuffer(this.buffer);
        if (this.texture) gl.deleteTexture(this.texture);
        if (this.vao) gl.deleteVertexArray(this.vao);
        this.releaseCoverage();
        this.canvas.removeEventListener('webglcontextlost', this.onLost);
        this.canvas.removeEventListener('webglcontextrestored', this.onRestored);
        this.atlasPixels = null;
        this.atlasMemory = null;
        this.disposed = true;
        this.worldBytes = 0;
        // Deleting objects leaves the context and driver caches alive until GC.
        // Retire this owner's context now, including after failed initialization.
        if (!gl.isContextLost()) gl.getExtension('WEBGL_lose_context')?.loseContext();
    }
}
