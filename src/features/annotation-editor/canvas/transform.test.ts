import { describe, expect, it } from "vitest";
import type { MosaicShape } from "../model/document";
import { arrow, marker, rect, text } from "../model/testing";
import { bakeIntegerRect, bakeScale, moveShape } from "./transform";

describe("transform", () => {
  it("moves every kind of shape", () => {
    const by = { x: 5, y: -10 };
    expect(moveShape(arrow("a"), by)).toMatchObject({
      start: { x: 15, y: 0 },
      end: { x: 105, y: 40 },
    });
    expect(moveShape(rect("r"), by).rect).toEqual({ x: 25, y: 20, width: 200, height: 100 });
    expect(moveShape(text("t"), by).position).toEqual({ x: 45, y: 30 });
    expect(moveShape(marker("m", 1), by).center).toEqual({ x: 305, y: 190 });
    const mosaic: MosaicShape = {
      id: "b",
      kind: "mosaic",
      rect: { x: 0, y: 20, width: 10, height: 10 },
      blockSize: 16,
    };
    expect(moveShape(mosaic, by).rect).toEqual({ x: 5, y: 10, width: 10, height: 10 });
  });

  it("folds the scale into the size, flips included", () => {
    expect(bakeScale({ x: 10, y: 20, width: 100, height: 50, scaleX: 1.5, scaleY: 2 })).toEqual({
      x: 10,
      y: 20,
      width: 150,
      height: 100,
    });
    expect(bakeScale({ x: 10, y: 20, width: 100, height: 50, scaleX: -0.5, scaleY: 1 })).toEqual({
      x: -40,
      y: 20,
      width: 50,
      height: 50,
    });
  });

  it("keeps mosaics in whole pixels inside the image", () => {
    const image = { width: 100, height: 100 };
    expect(
      bakeIntegerRect({ x: 90.4, y: 0, width: 20, height: 10.6, scaleX: 1, scaleY: 1 }, image),
    ).toEqual({
      x: 90,
      y: 0,
      width: 10,
      height: 11,
    });
    expect(
      bakeIntegerRect({ x: 200, y: 0, width: 20, height: 10, scaleX: 1, scaleY: 1 }, image),
    ).toBeNull();
  });
});
