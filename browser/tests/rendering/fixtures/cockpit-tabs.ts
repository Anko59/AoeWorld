import type { Page } from "@playwright/test";

// The far-view drawer is one panel with icon tabs; open a tab without toggling it shut.
export async function openTab(page: Page, id: string) {
  const tab = page.locator(`[data-tab="${id}"]`);
  if ((await tab.getAttribute("aria-selected")) !== "true") await tab.click();
}
