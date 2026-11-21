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
  await expect(cell("Militia").locator(".command-note")).toHaveText("");
  await expect(cell("Archer")).toBeDisabled();
  await expect(cell("Archer").locator(".command-note")).toHaveText(
    "Needs building",
  );
  await expect(cell("Archer")).toHaveAttribute(
    "aria-description",
    "Needs building",
  );
  const longName = cell("Two-Handed Swordsman").locator(".command-label");
  await expect(longName).toHaveCSS("white-space", "normal");
  const nameSize = await longName.evaluate((node) =>
    Number.parseFloat(getComputedStyle(node).fontSize),
  );
  expect(nameSize).toBeGreaterThanOrEqual(12);
  const drawer = page.locator('[data-body="train"]');
  const scroll = await drawer.evaluate((node) => {
    const action = node.querySelector(".far-act");
    if (!action) throw new Error("Training action missing");
    const before = action.getBoundingClientRect().top;
    node.scrollTop = node.scrollHeight;
    return {
      distance: node.scrollTop,
      before,
      after: action.getBoundingClientRect().top,
    };
  });
  expect(scroll.distance).toBeGreaterThan(100);
  expect(Math.abs(scroll.after - scroll.before)).toBeLessThan(2);
  await page.locator("#units-here").getByRole("button", { name: "10" }).click();
  await expect(page.getByLabel("Unit count")).toHaveValue("10");
  await expect(
    page.getByRole("button", { name: "Units here", exact: true }),
  ).toHaveText("Militia × 10 here");

  // Availability codes 4 and 11 of `building_ui/availability.rs`: a producer that
  // is researching or unfinished must say so rather than show a blank note.
  const notes = await page.evaluate(async () => {
    const { reasons } = (await import(
      "/navigation/production.js" as string
    )) as {
      reasons: string[];
    };
    return [reasons[4], reasons[11]];
  });
  expect(notes).toEqual(["Researching", "Under construction"]);
});
