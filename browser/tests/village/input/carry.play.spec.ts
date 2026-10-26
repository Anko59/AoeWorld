import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { inputFixture } from "./fixture.js";
import { forceCanvas } from "../../rendering/backends.js";

test("carry indicators toggle beside the grid, by L, and persist after reload", async ({
  page,
}, info) => {
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer, [], false, 0, undefined, {
    carried: true,
  });
  const toggle = page.locator("#carry-indicators");
  const badge = page.locator("#carried-loads .load-badge");
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  expect(
    await toggle.evaluate(
      (button) =>
        button.previousElementSibling?.id === "grid" ||
        button.nextElementSibling?.id === "grid",
    ),
  ).toBe(true);
  await expect(badge).toBeVisible();
  await expect(badge).toContainText("7");

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await expect(badge).toBeHidden();
  // Several real HUD updates must not repopulate visible indicators.
  await page.waitForTimeout(600);
  await expect(badge).toBeHidden();
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem("aoeworld.carryIndicators")),
    )
    .toBe("false");
  await page.reload();
  await expect(page.locator("#connection")).toHaveText("connected");
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await expect(badge).toBeHidden();
  await page.keyboard.press("l");
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem("aoeworld.carryIndicators")),
    )
    .toBe("true");
  await page.keyboard.press("l");
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
});

test("carry toggle tolerates unavailable storage and leaves text input alone", async ({
  page,
}, info) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => {
    for (const method of ["getItem", "setItem"]) {
      Object.defineProperty(Storage.prototype, method, {
        value() {
          throw new DOMException("Storage disabled", "SecurityError");
        },
        configurable: true,
      });
    }
  });
  const renderer = info.project.name === "webgpu" ? "webgpu" : "webgl2";
  await inputFixture(page, renderer, [], false, 0, undefined, {
    carried: true,
  });
  const toggle = page.locator("#carry-indicators");
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("l");
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#carried-loads .load-badge")).toBeVisible();
  await page.evaluate(() => {
    const input = document.createElement("input");
    input.id = "carry-text-input";
    document.body.append(input);
    input.focus();
  });
  await page.keyboard.press("l");
  await expect(page.locator("#carry-text-input")).toHaveValue("l");
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  expect(errors).toEqual([]);
});

test("Canvas carry badges use the same button and hotkey", async ({ page }) => {
  await forceCanvas(page);
  await inputFixture(page, "canvas2d", [], false, 0, undefined, {
    carried: true,
  });
  const badge = page.locator("#carried-loads .load-badge");
  await expect(badge).toBeVisible();
  await page.locator("#carry-indicators").click();
  await expect(badge).toBeHidden();
  await page.keyboard.press("l");
  await expect(badge).toBeVisible();
});
