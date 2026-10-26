import { expect, type Page } from "@playwright/test";

// Qualification captures inspect prepared packages without exposing a second
// map-creation UI in the game. Player map creation is covered by setup specs.
export async function activatePackage(page: Page, hash: string) {
  const token = await page.evaluate(
    () => localStorage.getItem("aoeworld.resume-token") ?? "",
  );
  const response = await page.request.post(`/maps/${hash}`, {
    headers: { "X-AoeWorld-Controller-Token": token },
  });
  expect(response.ok()).toBeTruthy();
  return response.json() as Promise<{
    start_available: boolean;
    message?: string;
  }>;
}
