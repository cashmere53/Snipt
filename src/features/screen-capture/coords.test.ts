import { describe, expect, it } from "vitest";
import { cssToPhysical, intersect, pointerToPhysical, rectBetween } from "./coords";

describe("cssToPhysical", () => {
  it.each([
    [1.25, 1536, 1920],
    [1.25, 0.8, 1],
    [1.25, 100.4, 126],
    [1.5, 2560, 3840],
    [1.5, 333.33, 500],
    [1.5, 0.3, 0],
  ])("at %s × maps %s CSS px to %s physical px", (scale, css, physical) => {
    expect(cssToPhysical(css, scale)).toBe(physical);
  });

  it("lands on every physical pixel when walking CSS pixels at 1.25 ×", () => {
    // 4 CSS px are 5 physical px: each step moves 1 or 2, never 0 or 3.
    const steps = Array.from(
      { length: 40 },
      (_, i) => cssToPhysical(i + 1, 1.25) - cssToPhysical(i, 1.25),
    );
    expect(new Set(steps)).toEqual(new Set([1, 2]));
    expect(cssToPhysical(40, 1.25)).toBe(50);
  });
});

describe("pointerToPhysical", () => {
  const size = { width: 3840, height: 2160 };

  it("stops a point past an edge at that edge", () => {
    expect(pointerToPhysical(-20, 50, 1.5, size)).toEqual({ x: 0, y: 75 });
    expect(pointerToPhysical(3000, 1500, 1.5, size)).toEqual({ x: 3840, y: 2160 });
  });
});

describe("rectBetween", () => {
  it("normalizes a drag in any direction", () => {
    const rect = { x: 10, y: 20, width: 30, height: 40 };
    expect(rectBetween({ x: 10, y: 20 }, { x: 40, y: 60 })).toEqual(rect);
    expect(rectBetween({ x: 40, y: 60 }, { x: 10, y: 20 })).toEqual(rect);
    expect(rectBetween({ x: 40, y: 20 }, { x: 10, y: 60 })).toEqual(rect);
  });
});

describe("intersect", () => {
  it("keeps the part inside, or nothing", () => {
    const monitor = { x: -3840, y: 0, width: 3840, height: 2160 };
    expect(intersect({ x: -100, y: 10, width: 300, height: 200 }, monitor)).toEqual({
      x: -100,
      y: 10,
      width: 100,
      height: 200,
    });
    expect(intersect({ x: 0, y: 0, width: 50, height: 50 }, monitor)).toBeNull();
  });
});
