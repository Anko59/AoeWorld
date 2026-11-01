import { expect, type WebSocketRoute } from "@playwright/test";
import { test } from "../../server.js";
import { explorationFixture } from "./exploration-fixture.js";

test("server validation and disconnect reasons stay visible with a reload link", async ({
  page,
}) => {
  let active: WebSocketRoute | undefined;
  await explorationFixture(page, (socket) => {
    active = socket;
  });
  await page.goto("/?play=1#x=256&y=256");
  await expect(page.locator("#connection")).toHaveText("connected");
  const socket = active;
  if (!socket) throw new Error("Gameplay socket was not intercepted");
  // CommandAck { sequence: 1, RejectedInvalidTarget, applied_tick: 1 }.
  socket.send(Buffer.from([3, 1, 16, 1]));
  await expect(
    page.getByRole("alert").filter({ hasText: "Cannot attack:" }),
  ).toContainText("that target is unavailable, friendly or out of range.");
  const message =
    "The server could not encode a game update. Reload to reconnect.";
  const bytes = Buffer.from(message);
  // Current Postcard Error { code: 500, message }.
  socket.send(Buffer.from([6, 244, 3, bytes.length, ...bytes]));
  await expect(
    page.getByRole("alert").filter({ hasText: message }),
  ).toBeVisible();
  // Stop later handshakes so the reconnect reason must survive retry startup.
  await page.routeWebSocket("**/game/ws", (socket) => {
    socket.onMessage(() => {});
  });
  socket.close({ code: 1011, reason: "Invalid server game state" });
  await expect(page.locator("#connection")).toContainText(
    /reconnect|connecting|negotiating/,
  );
  await expect(
    page.getByRole("alert").filter({ hasText: message }),
  ).toBeVisible();
  await expect(page.locator("#game-error-message")).toContainText(
    "Invalid server game state",
  );
  await expect(page.getByRole("link", { name: "Reload game" })).toHaveAttribute(
    "href",
    /./,
  );
});
