import { expect } from "@playwright/test";
import { test } from "../../../server.js";
import { playerState, productionFixture } from "./fixture.js";
import { openTab } from "../../rendering/fixtures/cockpit-tabs.js";

test("Units here lists every trainable unit and says what each one needs, with nothing selected", async ({
  page,
}) => {
  // A Town Center (kind 0) and a Barracks (kind 6); nothing is selected.
  const { update } = await productionFixture(page, [
    { id: 1, kind: 0, x: 8184, y: 8180 },
    { id: 2, kind: 6, x: 8191, y: 8186 },
  ]);
  update(playerState(0, 0, 4, 100));
  await page.evaluate(() => {
    location.hash = "x=8192.5&y=8192.5&zoom=.04";
  });
  await expect(page.locator("#scene")).toHaveAttribute("data-lod", "markers");
  await openTab(page, "train");
  const grid = page.getByRole("group", { name: "Unit type" });
  const cell = (name: string) =>
    grid.getByRole("button", { name, exact: true });
  // Units that the redesign had dropped are back.
  for (const name of ["Steward", "Vassal", "Galley", "Fishing Ship"])
    await expect(cell(name)).toHaveCount(1);
  // Availability comes from the owned buildings, not from a selection.
  await expect(cell("Militia")).toBeEnabled();
  await expect(cell("Militia").locator(".far-cell-note")).toHaveText("");
  await expect(cell("Archer")).toBeDisabled();
  await expect(cell("Archer").locator(".far-cell-note")).toHaveText(
    "Needs building",
  );
});
