import { chromium } from "@playwright/test";
import readline from "node:readline";
import path from "node:path";
import fs from "node:fs/promises";
import process from "node:process";
import { PNG } from "pngjs";
import { URL } from "node:url";

const sessions = new Map();
let browser;
const base = process.env.AOE_QA_BASE_URL || "http://127.0.0.1:8080";
const reportDirectory = path.resolve("../reports/qa");
const browserArgs = [
  "--use-angle=swiftshader",
  "--enable-unsafe-swiftshader",
  "--enable-unsafe-webgpu",
  "--ignore-gpu-blocklist",
  "--enable-gpu",
  "--enable-features=Vulkan",
  "--use-vulkan=swiftshader",
  "--use-webgpu-adapter=swiftshader",
  "--disable-vulkan-surface",
];

function session(name) {
  const found = sessions.get(name);
  if (!found) throw new Error("unknown QA session");
  return found;
}

async function execute(name, args) {
  if (name === "open_session") {
    if (sessions.has(args.session) || sessions.size >= 3)
      throw new Error("session exists or session limit reached");
    browser ||= await chromium.launch({ headless: true, args: browserArgs });
    const context = await browser.newContext({
      viewport: { width: 1280, height: 720 },
    });
    if (args.capability === "webgpu-disabled") {
      await context.addInitScript(() => {
        Object.defineProperty(globalThis.navigator, "gpu", {
          value: undefined,
        });
      });
    }
    const page = await context.newPage();
    const diagnostics = [];
    page.on("console", (message) => {
      if (message.type() === "error")
        diagnostics.push(`console: ${message.text().slice(0, 300)}`);
    });
    page.on("pageerror", (error) =>
      diagnostics.push(`page: ${error.message.slice(0, 300)}`),
    );
    page.on("requestfailed", (request) =>
      diagnostics.push(`request: ${request.url().slice(0, 150)}`),
    );
    const url =
      args.configuration === "invalid-scenario"
        ? new URL("/?scenario=invalid-demo", base).href
        : base;
    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 15000 });
    const response = await page.request.get(`${base}/health`);
    if (!response.ok()) throw new Error("health endpoint is unavailable");
    const health = await response.json();
    sessions.set(args.session, { context, page, diagnostics });
    return {
      session: args.session,
      build: health.build,
      scenario: health.scenario,
      url: page.url(),
    };
  }
  const current = session(args.session);
  const { page } = current;
  if (name === "observe") {
    return {
      title: await page.title(),
      accessible: (await page.locator("body").ariaSnapshot()).slice(0, 12000),
      status: (await page.getByRole("status").innerText()).slice(0, 1000),
    };
  }
  if (name === "activate") {
    const control = page.getByRole(args.role, {
      name: args.label,
      exact: true,
    });
    if ((await control.count()) !== 1 || !(await control.isVisible()))
      throw new Error("visible control is not unique");
    await control.click({ timeout: 5000 });
    return { activated: args.label };
  }
  if (name === "select_scenario") {
    await page
      .getByLabel("Scenario")
      .selectOption(args.scenario, { timeout: 5000 });
    return { selected: args.scenario };
  }
  if (name === "canvas_input") {
    const canvas = page.locator("canvas[aria-label='Synthetic entity field']");
    if (args.action === "key") {
      await page.keyboard.press(args.key);
    } else {
      const box = await canvas.boundingBox();
      if (!box) throw new Error("canvas is not visible");
      const x = box.x + args.x;
      const y = box.y + args.y;
      if (args.action === "click") await page.mouse.click(x, y);
      else if (args.action === "move") await page.mouse.move(x, y);
      else if (args.action === "wheel") await page.mouse.wheel(0, args.delta);
    }
    return { action: args.action };
  }
  if (name === "wait_text") {
    await page
      .getByText(args.text, { exact: false })
      .first()
      .waitFor({ state: "visible", timeout: args.timeout_ms });
    return { visible: args.text };
  }
  if (name === "screenshot") {
    await fs.mkdir(reportDirectory, { recursive: true });
    const filename = `qa-${args.session}-${Date.now()}-${Math.random().toString(16).slice(2, 8)}.png`;
    const output = path.join(reportDirectory, filename);
    const canvas = page.locator("canvas[aria-label='Synthetic entity field']");
    const box = await canvas.boundingBox();
    if (!box) throw new Error("canvas is not visible");
    const pageBuffer = await page.screenshot({
      fullPage: true,
      timeout: 10000,
    });
    const pageImage = PNG.sync.read(pageBuffer);
    const left = Math.round(box.x);
    const top = Math.round(box.y);
    if (
      left < 0 ||
      top < 0 ||
      left + Math.round(box.width) > pageImage.width ||
      top + Math.round(box.height) > pageImage.height
    )
      throw new Error("canvas capture exceeds page bounds");
    let spritePixels = 0;
    for (let y = top; y < top + Math.round(box.height); y += 1) {
      for (let x = left; x < left + Math.round(box.width); x += 1) {
        const i = (y * pageImage.width + x) * 4;
        const r = pageImage.data[i];
        const g = pageImage.data[i + 1];
        const b = pageImage.data[i + 2];
        if (
          (b > 150 && b > r + 50 && b > g + 20) ||
          (r > 200 && g > 80 && g < 190 && b < 150) ||
          (g > 150 && g > r + 30 && g > b + 20) ||
          (r > 180 && g > 140 && b < 130)
        )
          spritePixels += 1;
      }
    }
    await fs.writeFile(output, pageBuffer);
    return { path: output, canvas_sprite_pixels: spritePixels };
  }
  if (name === "diagnostics")
    return { messages: current.diagnostics.slice(-20) };
  if (name === "close_session") {
    await current.context.close();
    sessions.delete(args.session);
    return { closed: args.session };
  }
  throw new Error("unsupported browser action");
}

const lines = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});
for await (const line of lines) {
  try {
    const request = JSON.parse(line);
    const result = await execute(request.name, request.args);
    process.stdout.write(`${JSON.stringify({ ok: true, result })}\n`);
  } catch (error) {
    process.stdout.write(
      `${JSON.stringify({ ok: false, error: String(error) })}\n`,
    );
  }
}
for (const item of sessions.values()) await item.context.close();
await browser?.close();
