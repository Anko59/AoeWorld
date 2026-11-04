import { expect } from "@playwright/test";
import { test } from "../../../../server.js";

test("far map draws road plots only with the Roads and bridges layer enabled", async ({page}) => {
  await page.route("**/road-layer-test", route => route.fulfill({contentType:"text/html",body:"<canvas id='test' width='128' height='128'></canvas>"}));
  await page.goto("/road-layer-test");
  const result = await page.evaluate(async () => {
    // @ts-expect-error Runtime module is served by the game.
    const {initTokens} = await import("/navigation/tokens.js");
    // @ts-expect-error Runtime module is served by the game.
    const {initLayers} = await import("/navigation/layers.js");
    const layers = initLayers(() => {}); layers.show(true);
    const tokens = initTokens();
    const canvas = document.querySelector("canvas")!;
    const context = canvas.getContext("2d")!;
    const project = ([x,y]:number[]) => [x*16,y*16];
    tokens.update([],[],[2,2,5,5,23]);
    const alpha = () => context.getImageData(48,48,1,1).data[3];
    document.dispatchEvent(new KeyboardEvent("keydown",{key:"F6",bubbles:true}));
    tokens.draw(context,project,layers.enabled,1,128,128);
    const hidden = alpha();
    document.dispatchEvent(new KeyboardEvent("keydown",{key:"F6",bubbles:true}));
    tokens.draw(context,project,layers.enabled,1,128,128);
    const visible = alpha();
    context.clearRect(0,0,128,128); tokens.update([],[],[]);
    tokens.draw(context,project,{roads:true},1,128,128);
    return {hidden,visible,removed:alpha()};
  });
  expect(result).toEqual({hidden:0,visible:255,removed:0});
});
