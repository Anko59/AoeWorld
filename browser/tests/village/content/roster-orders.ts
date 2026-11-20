import { expect, type Page } from "@playwright/test";
import { postcardFields } from "../../wire/combat.js";
import { freshHud } from "../fixtures/resources.js";

export function required<T>(value: T | null | undefined, label: string): T {
  if (value == null) throw new Error(`${label} is missing`);
  return value;
}

export async function waitForGround(
  page: Page,
  point: { x: number; y: number },
) {
  await page.mouse.move(point.x, point.y);
  await freshHud(page);
  await expect(page.locator("#tile-inspection")).toHaveText(
    /^tile -?\d+, -?\d+: (movement level|diagnostic ground)/,
  );
}

export async function observeOrders(page: Page) {
  type Order = [tag: number, sequence: number, entity: number, value: number];
  const orders: Order[] = [];
  const acks = new Map<number, number>();
  page.on("websocket", (socket) => {
    socket.on("framesent", ({ payload }) => {
      if (typeof payload === "string") return;
      const [tag = -1, sequence = -1, entity = -1, value = -1] = postcardFields(
        payload,
        4,
      );
      if (tag === 2 || tag === 5 || tag === 7 || tag === 32) {
        acks.delete(sequence);
        orders.push([tag, sequence, entity, value]);
      }
    });
    socket.on("framereceived", ({ payload }) => {
      if (typeof payload === "string") return;
      const [tag, sequence, result] = postcardFields(payload, 3);
      if (tag === 3 && sequence !== undefined && result !== undefined)
        acks.set(sequence, result);
    });
  });
  return async (
    tag: number,
    entity: number,
    action: () => Promise<void>,
    result = 0,
  ) => {
    const from = orders.length;
    await action();
    let order: Order | undefined;
    await expect
      .poll(
        () => {
          order = orders
            .slice(from)
            .find((row) => row[0] === tag && row[2] === entity);
          return order === undefined ? undefined : acks.get(order[1]);
        },
        { message: `order ${tag} from entity ${entity}` },
      )
      .toBe(result);
    return required(order, `order ${tag} from entity ${entity}`)[3];
  };
}
