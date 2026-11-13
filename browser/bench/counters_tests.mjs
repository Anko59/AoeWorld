import test from "node:test";
import assert from "node:assert/strict";
import { installCounters } from "./counters.mjs";

test("counts submitted work only inside a frame and preserves calls", () => {
  const calls = [];
  const gl = Object.fromEntries(
    [
      "getError",
      "texSubImage3D",
      "bufferSubData",
      "bufferData",
      "drawArraysInstanced",
    ].map((name) => [
      name,
      function (...args) {
        calls.push([this, name, args]);
        return 7;
      },
    ]),
  );
  const counter = installCounters(gl);
  assert.equal(gl.getError(), 7);
  counter.reset();
  assert.equal(gl.getError(), 7);
  gl.texSubImage3D(0, 0, 0, 0, 0, 2, 3, 1, 0, 0, new Uint8Array(24));
  gl.bufferSubData(0, 0, new Uint8Array(64));
  gl.bufferData(0, 128, 0);
  gl.drawArraysInstanced(0, 0, 6, 5);
  assert.deepEqual(counter.take(), [1, 1, 24, 64, 128, 1, 5]);
  assert.equal(calls.length, 6);
  assert.equal(calls.at(-1)[0], gl);
  counter.reset();
  assert.deepEqual(counter.take(), Array(7).fill(0));
});

test("typed-array upload ranges count bytes rather than elements", () => {
  const gl = Object.fromEntries(
    [
      "getError",
      "texSubImage3D",
      "bufferSubData",
      "bufferData",
      "drawArraysInstanced",
    ].map((name) => [name, () => {}]),
  );
  const counter = installCounters(gl);
  counter.reset();
  gl.bufferSubData(0, 0, new Float32Array(12), 2, 3);
  assert.equal(counter.take()[3], 12);
});

test(
  "real WebGL2 submissions retain pixels and report the submitted work",
  { timeout: 30000 },
  async () => {
    const { chromium } = await import("@playwright/test");
    const browser = await chromium.launch({
      headless: true,
      args: [
        "--no-sandbox",
        "--use-angle=swiftshader",
        "--enable-unsafe-swiftshader",
      ],
    });
    try {
      const page = await browser.newPage();
      await page.setContent('<canvas width="4" height="4"></canvas>');
      await page.addScriptTag({
        content: `window.installFrameCounters = ${installCounters.toString()};`,
      });
      const actual = await page.evaluate(() => {
        const gl = globalThis.document
          .querySelector("canvas")
          .getContext("webgl2", { alpha: false, antialias: false });
        if (!gl) throw new Error("real WebGL2 context unavailable");
        const program = gl.createProgram();
        for (const [type, source] of [
          [
            gl.VERTEX_SHADER,
            "#version 300 es\nvoid main(){ vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2)); gl_Position=vec4(p*2.0-1.0,0.0,1.0); }",
          ],
          [
            gl.FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float; out vec4 color; void main(){ color=vec4(0.0,1.0,0.0,1.0); }",
          ],
        ]) {
          const shader = gl.createShader(type);
          gl.shaderSource(shader, source);
          gl.compileShader(shader);
          if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS))
            throw new Error(gl.getShaderInfoLog(shader));
          gl.attachShader(program, shader);
        }
        gl.linkProgram(program);
        if (!gl.getProgramParameter(program, gl.LINK_STATUS))
          throw new Error(gl.getProgramInfoLog(program));
        gl.useProgram(program);
        gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
        gl.bindTexture(gl.TEXTURE_2D_ARRAY, gl.createTexture());
        gl.texStorage3D(gl.TEXTURE_2D_ARRAY, 1, gl.RGBA8, 2, 2, 1);
        const counter = globalThis.installFrameCounters(
          globalThis.WebGL2RenderingContext.prototype,
        );
        counter.reset();
        gl.texSubImage3D(
          gl.TEXTURE_2D_ARRAY,
          0,
          0,
          0,
          0,
          2,
          2,
          1,
          gl.RGBA,
          gl.UNSIGNED_BYTE,
          new Uint8Array(16),
        );
        gl.bufferData(gl.ARRAY_BUFFER, 64, gl.DYNAMIC_DRAW);
        gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Float32Array([1, 2, 3, 4]));
        gl.drawArraysInstanced(gl.TRIANGLES, 0, 3, 2);
        const error = gl.getError();
        const counts = counter.take();
        const pixel = new Uint8Array(4);
        gl.readPixels(1, 1, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        if (gl.getError() !== gl.NO_ERROR)
          throw new Error("WebGL pixel read failed");
        return { error, counts, pixel: [...pixel] };
      });
      assert.deepEqual(actual, {
        error: 0,
        counts: [1, 1, 16, 16, 64, 1, 2],
        pixel: [0, 255, 0, 255],
      });
    } finally {
      await browser.close();
    }
  },
);
