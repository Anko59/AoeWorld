import { mkdir, readFile } from "node:fs/promises";
import type { Page } from "@playwright/test";

/** Optional visual proof uses this checkout's `make dev DEV_PORT=8143`. */
export async function automationBase(): Promise<string> {
  const capture = await readFile(
    "../.cache/eco-automation/capture.json",
    "utf8",
  )
    .then((text) => JSON.parse(text) as { url: string })
    .catch(() => null);
  if (capture && capture.url !== "http://127.0.0.1:8143/")
    throw new Error("Unexpected economy automation capture origin");
  return capture?.url ?? "/";
}

export async function captureAutomation(
  page: Page,
  base: string,
  name: string,
) {
  if (base === "/") return;
  await mkdir("../.cache/eco-automation", { recursive: true });
  await page.screenshot({ path: `../.cache/eco-automation/${name}.png` });
}
