import { describe, expect, it } from "vitest";
import type { Point } from "./document";
import { constrainSquare, normalizeRect, snapAngle45, toIntegerRect } from "./geometry";

const close = (p: Point) => ({ x: Math.round(p.x * 1e6) / 1e6, y: Math.round(p.y * 1e6) / 1e6 });

describe("normalizeRect", () => {
  it("spans the two corners in any drag direction", () => {
    const rect = { x: 10, y: 20, width: 30, height: 40 };
    expect(normalizeRect({ x: 10, y: 20 }, { x: 40, y: 60 })).toEqual(rect);
    expect(normalizeRect({ x: 40, y: 60 }, { x: 10, y: 20 })).toEqual(rect);
    expect(normalizeRect({ x: 40, y: 20 }, { x: 10, y: 60 })).toEqual(rect);
  });
});

describe("snapAngle45", () => {
  const start = { x: 100, y: 100 };

  it.each([
    ["right", { x: 150, y: 110 }, { x: 150, y: 100 }],
    ["left", { x: 40, y: 95 }, { x: 40, y: 100 }],
    ["down", { x: 104, y: 180 }, { x: 100, y: 180 }],
    ["up", { x: 97, y: 30 }, { x: 100, y: 30 }],
    ["down right", { x: 150, y: 140 }, { x: 145, y: 145 }],
    ["up right", { x: 140, y: 50 }, { x: 145, y: 55 }],
    ["down left", { x: 50, y: 140 }, { x: 55, y: 145 }],
    ["up left", { x: 60, y: 50 }, { x: 55, y: 55 }],
  ])("snaps a drag %s", (_, current, expected) => {
    expect(close(snapAngle45(start, current))).toEqual(expected);
  });

  it("stays at the start when the pointer has not moved", () => {
    expect(snapAngle45(start, start)).toEqual(start);
  });
});

describe("constrainSquare", () => {
  const start = { x: 100, y: 100 };

  it.each([
    ["down right", { x: 130, y: 120 }, { x: 100, y: 100 }],
    ["down left", { x: 80, y: 130 }, { x: 70, y: 100 }],
    ["up right", { x: 110, y: 70 }, { x: 100, y: 70 }],
    ["up left", { x: 70, y: 90 }, { x: 70, y: 70 }],
  ])("makes a square on the longer side, dragging %s", (_, current, corner) => {
    expect(constrainSquare(start, current)).toEqual({ ...corner, width: 30, height: 30 });
  });
});

describe("toIntegerRect", () => {
  const bounds = { width: 100, height: 50 };

  it("rounds the edges to whole pixels", () => {
    expect(toIntegerRect({ x: 10.4, y: 5.6, width: 20.2, height: 10.3 }, bounds)).toEqual({
      x: 10,
      y: 6,
      width: 21,
      height: 10,
    });
  });

  it("clamps into the image", () => {
    expect(toIntegerRect({ x: -20, y: 40, width: 50, height: 30 }, bounds)).toEqual({
      x: 0,
      y: 40,
      width: 30,
      height: 10,
    });
    expect(toIntegerRect({ x: 90, y: -5, width: 40, height: 100 }, bounds)).toEqual({
      x: 90,
      y: 0,
      width: 10,
      height: 50,
    });
  });

  it("accepts negative sizes", () => {
    expect(toIntegerRect({ x: 30, y: 30, width: -10, height: -20 }, bounds)).toEqual({
      x: 20,
      y: 10,
      width: 10,
      height: 20,
    });
  });

  it("is null without area", () => {
    expect(toIntegerRect({ x: 10, y: 10, width: 0, height: 10 }, bounds)).toBeNull();
    expect(toIntegerRect({ x: 10, y: 10, width: 0.3, height: 10 }, bounds)).toBeNull();
    expect(toIntegerRect({ x: 120, y: 10, width: 10, height: 10 }, bounds)).toBeNull();
    expect(toIntegerRect({ x: 10, y: -30, width: 10, height: 20 }, bounds)).toBeNull();
  });
});
