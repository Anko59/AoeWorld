import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { nativeMenu } from "../fixtures/native-menu.js";

test("General and Steward ranges share art, values and keyboard and pointer input", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await nativeMenu(page);
  await page.addScriptTag({
    path: new URL("../fixtures/selection-states.js", import.meta.url).pathname,
  });
  await page.waitForFunction(() => "__h" in window);
  await expect(page.locator("#bottom-panel")).toHaveAttribute(
    "data-slider-art",
    "native",
  );
  const inputs = page.locator(
    "#selection .menu-slider input[type=range]:visible",
  );
  for (const section of ["general", "steward"]) {
    await page.evaluate(
      (section) => Reflect.get(window, "__h")[section]("trainer", 0),
      section,
    );
    await expect(inputs).toHaveCount(5);
    for (const slider of await inputs.all()) {
      await expect(slider).toHaveCSS("appearance", "none");
      const before = Number(await slider.inputValue());
      const step = Number(await slider.getAttribute("step"));
      await slider.press(before === 100 ? "ArrowLeft" : "ArrowRight");
      const value = before + (before === 100 ? -step : step);
      await expect(slider).toHaveValue(String(value));
      await expect(slider.locator("+ output")).toHaveText(
        section === "general" ? `${value}%` : String(value),
      );
      const box = await slider.boundingBox();
      if (!box) throw new Error("Slider missing");
      const fraction = value < 50 ? 0.85 : 0.15;
      await page.mouse.click(
        box.x + box.width * fraction,
        box.y + box.height / 2,
      );
      await expect(slider).not.toHaveValue(String(value));
    }
  }
});
