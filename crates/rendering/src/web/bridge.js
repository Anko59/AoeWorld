// Standalone prototype, not wired: Rust supplies its authoritative compacted WGSL.
// Graphics calls only. Scene/depth normalization and atlas admission stay in Rust.
const STRIDE = 112;
const MAX_BYTES = 64 * 1024 * 1024;
const MAX_CAPACITY = Math.floor(MAX_BYTES / STRIDE);
const GAME_SIDE = 2048;
const GAME_PAGES = 3;
const GAME_BYTES = GAME_SIDE * GAME_SIDE * GAME_PAGES * 4;
// Exact browser keys mapped from wgpu 30 Limits::defaults(), not adapter maxima.
const REQUIRED_LIMITS = Object.freeze({
  maxTextureDimension1D: 8192, maxTextureDimension2D: 8192,
  maxTextureDimension3D: 2048, maxTextureArrayLayers: 256,
  maxBindGroups: 4, maxBindGroupsPlusVertexBuffers: 24,
  maxBindingsPerBindGroup: 1000,
  maxDynamicUniformBuffersPerPipelineLayout: 8,
  maxDynamicStorageBuffersPerPipelineLayout: 4,
  maxSampledTexturesPerShaderStage: 16, maxSamplersPerShaderStage: 16,
  maxStorageBuffersPerShaderStage: 8, maxStorageTexturesPerShaderStage: 4,
  maxUniformBuffersPerShaderStage: 12, maxUniformBufferBindingSize: 65536,
  maxStorageBufferBindingSize: 128 * 1024 * 1024,
  minUniformBufferOffsetAlignment: 256, minStorageBufferOffsetAlignment: 256,
  maxVertexBuffers: 8, maxBufferSize: 256 * 1024 * 1024,
  maxVertexAttributes: 16, maxVertexBufferArrayStride: 2048,
  maxInterStageShaderVariables: 16, maxColorAttachments: 8,
  maxColorAttachmentBytesPerSample: 32,
  maxComputeWorkgroupStorageSize: 16384, maxComputeInvocationsPerWorkgroup: 256,
  maxComputeWorkgroupSizeX: 256, maxComputeWorkgroupSizeY: 256,
  maxComputeWorkgroupSizeZ: 64, maxComputeWorkgroupsPerDimension: 65535,
});
function capacity(n) {
  if (!Number.isSafeInteger(n) || n < 0 || n > MAX_CAPACITY) {
    throw `visible world layers exceed the ${MAX_BYTES}-byte WebGPU instance bound`;
  }
  return n;
}
function message(error) {
  return error instanceof Error ? error.message : String(error);
}
export async function createWebGpu(canvas, shaderSource, initialCapacity) {
  capacity(initialCapacity);
  if (initialCapacity === 0) throw "WebGPU initial instance capacity must be positive";
  let context;
  try { context = canvas.getContext("webgpu"); }
  catch (error) { throw `WebGPU surface: ${message(error)}`; }
  if (!context) throw "WebGPU surface: no WebGPU canvas context";
  const gpu = globalThis.navigator?.gpu;
  if (!gpu) throw "No WebGPU adapter: browser WebGPU API is unavailable";
  let adapter;
  try { adapter = await gpu.requestAdapter({}); }
  catch (error) { throw `No WebGPU adapter: ${message(error)}`; }
  if (!adapter) throw "No WebGPU adapter: requestAdapter returned null";
  let device;
  try {
    device = await adapter.requestDevice({
      requiredFeatures: [], requiredLimits: REQUIRED_LIMITS,
    });
  } catch (error) { throw `WebGPU device: ${message(error)}`; }
  let bridge;
  try {
    // wgpu starts [rgba8unorm, bgra8unorm] and swaps the preferred one first.
    const preferred = gpu.getPreferredCanvasFormat();
    const format = preferred === "bgra8unorm" ? preferred : "rgba8unorm";
    bridge = new AoeWebGpu(canvas, context, device, format,
      `BrowserWebGpu: ${adapter.info.description}`, shaderSource, initialCapacity);
    return bridge;
  } catch (error) {
    if (bridge) bridge.dispose();
    else { device.destroy(); context.unconfigure(); }
    throw `WebGPU initialization: ${message(error)}`;
  }
}
export class AoeWebGpu {
  #canvas; #context; #device; #format; #label;
  #pipeline; #layout; #buffer; #bindGroup; #atlas; #atlasView; #sampler; #depth;
  #width; #height; #capacity; #atlasSide = 8; #atlasPages = 1;
  #configureFailed = false; #disposed = false; #deviceDestroyed = false; #preparedCount = 0;
  constructor(canvas, context, device, format, label, shaderSource, initialCapacity) {
    this.#canvas = canvas; this.#context = context; this.#device = device;
    this.#format = format; this.#label = label;
    this.#width = Math.max(1, canvas.width); this.#height = Math.max(1, canvas.height);
    this.#configure();
    const shader = device.createShaderModule({ label: "synthetic sprites", code: shaderSource });
    this.#layout = device.createBindGroupLayout({
      label: "sprite instance and atlas layout", entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage", hasDynamicOffset: false } },
        { binding: 1, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "2d-array", multisampled: false } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
    });
    this.#pipeline = device.createRenderPipeline({
      label: "sprite pipeline",
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.#layout] }),
      vertex: { module: shader, entryPoint: "vs_main", buffers: [] },
      fragment: { module: shader, entryPoint: "fs_main", targets: [{
        format, writeMask: GPUColorWrite.ALL,
        blend: {
          color: { srcFactor: "src-alpha", dstFactor: "one-minus-src-alpha", operation: "add" },
          alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
        },
      }] },
      primitive: { topology: "triangle-list", frontFace: "ccw", cullMode: "none" },
      depthStencil: { format: "depth24plus", depthWriteEnabled: true, depthCompare: "less-equal" },
      multisample: { count: 1, mask: 0xffffffff, alphaToCoverageEnabled: false },
    });
    this.#capacity = initialCapacity;
    this.#buffer = this.#newBuffer(initialCapacity);
    const pixels = new Uint8Array(8 * 8 * 4);
    for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
      const p = (y * 8 + x) * 4;
      pixels[p] = pixels[p + 1] = pixels[p + 2] = 255;
      pixels[p + 3] = x >= 1 && x < 7 && y >= 1 && y < 7 ? 255 : 0;
    }
    this.#atlas = this.#newAtlas(8, 1);
    this.#writeAtlas(this.#atlas, pixels, 8, 1);
    this.#atlasView = this.#atlas.createView({ dimension: "2d-array" });
    this.#sampler = this.#newSampler();
    this.#bindGroup = this.#newBindGroup(this.#buffer, this.#atlasView, this.#sampler);
    this.#depth = this.#newDepth();
  }
  get adapterLabel() { return this.#label; }
  get format() { return this.#format; }
  #live() {
    if (this.#disposed || this.#deviceDestroyed) throw "WebGPU surface lost; reload to restore it";
  }
  destroyDevice() {
    if (!this.#deviceDestroyed) {
      this.#deviceDestroyed = true; this.#device.destroy();
    }
  }
  #configure() {
    this.#canvas.width = this.#width; this.#canvas.height = this.#height;
    try {
      this.#context.configure({ device: this.#device, format: this.#format,
        usage: GPUTextureUsage.RENDER_ATTACHMENT, alphaMode: "opaque" });
      this.#configureFailed = false;
    } catch (_) { this.#configureFailed = true; }
  }
  #newBuffer(n) {
    return this.#device.createBuffer({ label: "sprite instances", size: n * STRIDE,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST, mappedAtCreation: false });
  }
  #newAtlas(side, pages) {
    return this.#device.createTexture({ label: "sprite atlas", size: [side, side, pages],
      mipLevelCount: 1, sampleCount: 1, dimension: "2d", format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST, viewFormats: [] });
  }
  #writeAtlas(texture, pixels, side, pages) {
    this.#device.queue.writeTexture({ texture, mipLevel: 0, origin: [0, 0, 0] },
      pixels, { offset: 0, bytesPerRow: side * 4, rowsPerImage: side }, [side, side, pages]);
  }
  #newSampler() {
    return this.#device.createSampler({ magFilter: "nearest", minFilter: "nearest",
      mipmapFilter: "nearest", addressModeU: "clamp-to-edge", addressModeV: "clamp-to-edge",
      addressModeW: "clamp-to-edge", lodMinClamp: 0, lodMaxClamp: 32, maxAnisotropy: 1 });
  }
  #newBindGroup(buffer, view, sampler) {
    return this.#device.createBindGroup({ layout: this.#layout, entries: [
      { binding: 0, resource: { buffer } }, { binding: 1, resource: view },
      { binding: 2, resource: sampler },
    ] });
  }
  #newDepth() {
    return this.#device.createTexture({ label: "sprite depth", size: [this.#width, this.#height, 1],
      mipLevelCount: 1, sampleCount: 1, dimension: "2d", format: "depth24plus",
      usage: GPUTextureUsage.RENDER_ATTACHMENT, viewFormats: [] });
  }
  ensureCapacity(required) {
    this.#live(); capacity(required);
    if (required <= this.#capacity) return;
    const buffer = this.#newBuffer(required);
    let group;
    try { group = this.#newBindGroup(buffer, this.#atlasView, this.#sampler); }
    catch (error) { buffer.destroy(); throw error; }
    this.#buffer.destroy(); this.#buffer = buffer;
    this.#bindGroup = group; this.#capacity = required;
  }
  uploadAtlas(pixels) {
    this.#live();
    if (!(pixels instanceof Uint8Array) || pixels.byteLength !== GAME_BYTES) {
      throw "Invalid game atlas size";
    }
    const limits = this.#device.limits;
    if (limits.maxTextureDimension2D < GAME_SIDE || limits.maxTextureArrayLayers < GAME_PAGES) {
      throw "WebGPU cannot support the bounded three-page atlas";
    }
    const texture = this.#newAtlas(GAME_SIDE, GAME_PAGES);
    let view, sampler, group;
    try {
      // Synchronous API snapshot: no retained caller view, copy, or await.
      this.#writeAtlas(texture, pixels, GAME_SIDE, GAME_PAGES);
      view = texture.createView({ dimension: "2d-array" }); sampler = this.#newSampler();
      group = this.#newBindGroup(this.#buffer, view, sampler);
    } catch (error) { texture.destroy(); throw error; }
    this.#atlas.destroy(); this.#atlas = texture; this.#atlasView = view;
    this.#sampler = sampler; this.#bindGroup = group;
    this.#atlasSide = GAME_SIDE; this.#atlasPages = GAME_PAGES;
  }
  resize(width, height) {
    this.#live();
    if (!Number.isInteger(width) || !Number.isInteger(height) || width < 0 || height < 0) {
      throw "Invalid WebGPU backing dimensions";
    }
    if (!width || !height || (width === this.#width && height === this.#height)) return;
    this.#width = width; this.#height = height; this.#configure();
    this.#depth.destroy(); this.#depth = this.#newDepth();
  }
  writePreparedInstances(bytes) {
    this.#live();
    if (!(bytes instanceof Uint8Array) || bytes.byteLength % STRIDE) {
      throw "Invalid WebGPU instance packet";
    }
    const count = capacity(bytes.byteLength / STRIDE);
    this.ensureCapacity(count);
    if (count) this.#device.queue.writeBuffer(this.#buffer, 0, bytes);
    this.#preparedCount = count;
    return count;
  }
  #encodePass(encoder, view, count, clear, depthView) {
    if (!Number.isInteger(count) || count < 0 || count > this.#preparedCount) {
      throw "Invalid WebGPU prepared instance count";
    }
    if (clear.length !== 4 || !Array.from(clear).every(Number.isFinite)) {
      throw "Invalid WebGPU clear color";
    }
    const pass = encoder.beginRenderPass({ colorAttachments: [{ view,
      clearValue: { r: clear[0], g: clear[1], b: clear[2], a: clear[3] },
      loadOp: "clear", storeOp: "store" }],
      depthStencilAttachment: { view: depthView, depthClearValue: 1,
        depthLoadOp: "clear", depthStoreOp: "store" } });
    pass.setPipeline(this.#pipeline); pass.setBindGroup(0, this.#bindGroup);
    pass.draw(6, count, 0, 0); pass.end();
  }
  renderPreparedInstances(bytes, clear) {
    const count = this.writePreparedInstances(bytes);
    if (this.#configureFailed) throw "WebGPU surface lost; reload to restore it";
    let frame;
    try { frame = this.#context.getCurrentTexture(); }
    catch (_) { throw "WebGPU surface lost; reload to restore it"; }
    const encoder = this.#device.createCommandEncoder({ label: "sprites" });
    this.#encodePass(encoder, frame.createView(), count, clear, this.#depth.createView());
    this.#device.queue.submit([encoder.finish()]);
    return true; // Browser presentation follows submit; no separate present API.
  }
  diagnostics() {
    this.#live();
    return { capacity: this.#capacity, gpuBufferBytes: this.#capacity * STRIDE,
      persistentGpuResources: 7, atlasPages: this.#atlasPages, atlasUploads: 1,
      atlasBytes: this.#atlasSide * this.#atlasSide * this.#atlasPages * 4,
      width: this.#width, height: this.#height, format: this.#format };
  }
  // Explicit test resource factory; no framebuffer/readback storage in production.
  // Uses the SAME encodePass, shader, depth attachment, bindgroup and upload path.
  createTestReadback(probeCount) {
    this.#live();
    if (!Number.isInteger(probeCount) || probeCount < 1 || probeCount > 5) {
      throw "bounded GPU readback probe count";
    }
    if (this.#width !== 128 || this.#height !== 128) throw "GPU readback requires 128x128 backing";
    const target = this.#device.createTexture({ label: "splat pixel test",
      size: [128, 128, 1], mipLevelCount: 1, sampleCount: 1, dimension: "2d",
      format: this.#format, usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC });
    let output;
    try { output = this.#device.createBuffer({ label: "splat pixel readback",
      size: probeCount * 256, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ }); }
    catch (error) { target.destroy(); throw error; }
    const view = target.createView(), depthView = this.#depth.createView();
    let busy = false, disposed = false;
    return {
      read: async (count, points, clear) => {
        this.#live();
        if (disposed || busy) throw "GPU readback is disposed or already mapping";
        if (points.length !== probeCount || !points.every(p => p.length === 2 &&
          p.every(n => Number.isInteger(n) && n >= 0 && n < 128))) {
          throw "Invalid GPU readback probe points";
        }
        busy = true;
        try {
          const encoder = this.#device.createCommandEncoder();
          this.#encodePass(encoder, view, count, clear, depthView);
          for (let i = 0; i < probeCount; i++) {
            encoder.copyTextureToBuffer({ texture: target, mipLevel: 0,
              origin: [points[i][0], points[i][1], 0] },
              { buffer: output, offset: i * 256, bytesPerRow: 256, rowsPerImage: 1 }, [1, 1, 1]);
          }
          this.#device.queue.submit([encoder.finish()]);
          await output.mapAsync(GPUMapMode.READ);
          const data = new Uint8Array(output.getMappedRange());
          const pixels = new Uint8Array(probeCount * 4);
          for (let i = 0; i < probeCount; i++) {
            const p = i * 256, q = i * 4, bgra = this.#format.startsWith("bgra8");
            pixels[q] = data[p + (bgra ? 2 : 0)]; pixels[q + 1] = data[p + 1];
            pixels[q + 2] = data[p + (bgra ? 0 : 2)]; pixels[q + 3] = data[p + 3];
          }
          return pixels; // Owned tiny result, never a mapped-buffer/WASM view.
        } finally { output.unmap(); busy = false; }
      },
      dispose: () => {
        if (!disposed) { disposed = true; output.destroy(); target.destroy(); }
      },
    };
  }
  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#buffer?.destroy(); this.#atlas?.destroy(); this.#depth?.destroy();
    this.#device.destroy(); this.#context.unconfigure();
  }
}
