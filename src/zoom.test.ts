import { describe, it } from "node:test";
import { deepStrictEqual, ok } from "node:assert";
import { fitTransform, zoomAtPoint } from "./zoom";

describe("cursor-centred image zoom", () => {
  it("keeps the same image point beneath an off-centre cursor", () => {
    const before = { scale: 2, x: -40, y: 65 };
    const cursor = { x: 280, y: -135 };
    const after = zoomAtPoint(before, cursor, 1.5);
    ok(Math.abs((cursor.x - before.x) / before.scale - (cursor.x - after.x) / after.scale) < 1e-9);
    ok(Math.abs((cursor.y - before.y) / before.scale - (cursor.y - after.y) / after.scale) < 1e-9);
  });
  it("returns to the same position after zooming in and back out", () => {
    const before = { scale: 2, x: 25, y: -50 }, cursor = { x: -175, y: 90 };
    deepStrictEqual(zoomAtPoint(zoomAtPoint(before, cursor, 2), cursor, 0.5), before);
  });
  it("limits magnification while preserving the cursor anchor", () => {
    deepStrictEqual(zoomAtPoint({ scale: 8, x: -100, y: 0 }, { x: 100, y: 0 }, 4), { scale: 16, x: -300, y: 0 });
  });
  it("recentres and fits when zooming all the way out", () => {
    deepStrictEqual(zoomAtPoint({ scale: 2, x: 500, y: -200 }, { x: 70, y: 80 }, 0.01), fitTransform);
  });
});
