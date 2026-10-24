import { describe, expect, it } from "vitest";
import { bandsAround } from "./CropOverlay";

describe("bandsAround", () => {
  const whole = { x: 0, y: 0, width: 100, height: 80 };

  it("dims the four sides around the crop", () => {
    expect(bandsAround({ x: 10, y: 20, width: 30, height: 40 }, whole)).toEqual([
      { x: 0, y: 0, width: 100, height: 20 },
      { x: 0, y: 60, width: 100, height: 20 },
      { x: 0, y: 20, width: 10, height: 40 },
      { x: 40, y: 20, width: 60, height: 40 },
    ]);
  });

  it("leaves out empty sides", () => {
    expect(bandsAround(whole, whole)).toEqual([]);
    expect(bandsAround({ x: 0, y: 0, width: 50, height: 80 }, whole)).toEqual([
      { x: 50, y: 0, width: 50, height: 80 },
    ]);
  });
});
