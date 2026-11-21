import { expect, type Page } from "@playwright/test";

export async function monitorCommands(page: Page) {
  await page.evaluate(() => {
    const menu = Reflect.get(window, "aoeBuildings");
    const send = menu.command;
    const calls: number[][] = [];
    Reflect.set(window, "menuOrders", calls);
    menu.bind((...args: number[]) => {
      calls.push(args);
      send(...args);
    });
  });
  const orders = () =>
    page.evaluate(() => {
      const calls: number[][] = Reflect.get(window, "menuOrders");
      return calls.splice(0);
    });
  const scene = page.locator("#scene");
  const buttons = page.locator("#command-grid button");
  const ids = () =>
    buttons.evaluateAll((commands) => commands.map(({ id }) => id));
  return async (selection: string) => {
    await orders();
    for (;;) {
      const shortcuts = await buttons.evaluateAll((commands) =>
        commands.map((button) => ({
          id: button.id,
          key: button.getAttribute("aria-keyshortcuts"),
        })),
      );
      const keys = shortcuts.map(({ key }) => key);
      expect(keys.every(Boolean), selection).toBe(true);
      expect(new Set(keys).size, selection).toBe(keys.length);
      for (const { id, key } of shortcuts) {
        // The page and building tabs update the grid instead of sending an order.
        if (
          [
            "more-commands",
            "previous-commands",
            "build-menu",
            "build-military-menu",
            "close-build-menu",
          ].includes(id)
        )
          continue;
        if (!key) throw new Error(`Missing shortcut: ${selection} ${id}`);
        const button = page.locator("#" + id);
        const enabled = await button.isEnabled();
        await scene.focus();
        await page.keyboard.press(key.length === 1 ? key.toLowerCase() : key);
        const viaKey = await orders();
        if (!enabled) {
          expect(viaKey, `${selection} ${id}`).toEqual([]);
          continue;
        }
        await button.click();
        expect(await orders(), `${selection} ${id}`).toEqual(viaKey);
        expect(viaKey).toHaveLength(1);
      }
      const next = page.locator("#more-commands");
      if (!(await next.count())) return;
      const before = await ids();
      await scene.focus();
      await page.keyboard.press("]");
      const after = await ids();
      expect(after, selection).not.toEqual(before);
      await page.locator("#previous-commands").click();
      expect(await ids(), selection).toEqual(before);
      await next.click();
      expect(await ids(), selection).toEqual(after);
      expect(await orders(), selection).toEqual([]);
    }
  };
}
