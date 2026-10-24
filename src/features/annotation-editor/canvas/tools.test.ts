import { describe, expect, it } from "vitest";
import { DEFAULT_STYLE } from "../model/style";
import { blank, marker } from "../model/testing";
import {
  dragResult,
  insideImage,
  isClickTool,
  isDragTool,
  markerAt,
  type ToolContext,
} from "./tools";

const ctx = (overrides: Partial<ToolContext> = {}): ToolContext => ({
  doc: blank(),
  style: DEFAULT_STYLE,
  newId: () => "new",
  ...overrides,
});

describe("drag tools", () => {
  it("draw arrows in the current style, snapped to 45° with Shift", () => {
    const free = dragResult("arrow", { x: 10, y: 10 }, { x: 50, y: 18 }, false, ctx(), "a");
    expect(free).toEqual({
      kind: "shape",
      shape: {
        id: "a",
        kind: "arrow",
        start: { x: 10, y: 10 },
        end: { x: 50, y: 18 },
        color: "#E53935",
        strokeWidth: 4,
      },
    });
    const snapped = dragResult("arrow", { x: 10, y: 10 }, { x: 50, y: 18 }, true, ctx(), "a");
    expect(
      snapped?.kind === "shape" && snapped.shape.kind === "arrow" && snapped.shape.end,
    ).toEqual({
      x: 50,
      y: 10,
    });
  });

  it("draw rectangles, squares with Shift", () => {
    const square = dragResult("rect", { x: 100, y: 100 }, { x: 60, y: 130 }, true, ctx(), "r");
    expect(square?.kind === "shape" && square.shape.kind === "rect" && square.shape.rect).toEqual({
      x: 60,
      y: 100,
      width: 40,
      height: 40,
    });
  });

  it("round mosaics and crops to whole pixels inside the image", () => {
    const mosaic = dragResult(
      "mosaic",
      { x: -5.4, y: 10.6 },
      { x: 40.2, y: 700 },
      false,
      ctx(),
      "m",
    );
    expect(mosaic).toEqual({
      kind: "shape",
      shape: {
        id: "m",
        kind: "mosaic",
        rect: { x: 0, y: 11, width: 40, height: 589 },
        blockSize: 16,
      },
    });
    expect(dragResult("crop", { x: 790.2, y: 5 }, { x: 900, y: 50 }, false, ctx(), "c")).toEqual({
      kind: "crop",
      rect: { x: 790, y: 5, width: 10, height: 45 },
    });
  });

  it("make nothing from clicks and slivers", () => {
    const at = { x: 10, y: 10 };
    const tools = ["arrow", "rect", "mosaic", "crop"] as const;
    expect(tools.map((tool) => dragResult(tool, at, { x: 11, y: 11 }, false, ctx(), "x"))).toEqual([
      null,
      null,
      null,
      null,
    ]);
    expect(dragResult("rect", at, { x: 200, y: 11 }, false, ctx(), "x")).toBeNull();
    expect(
      dragResult("mosaic", { x: 900, y: 10 }, { x: 950, y: 50 }, false, ctx(), "x"),
    ).toBeNull();
  });

  it("tell drag tools from click tools", () => {
    expect(["arrow", "rect", "mosaic", "crop"].every((t) => isDragTool(t as never))).toBe(true);
    expect(isDragTool("text")).toBe(false);
    expect(isClickTool("marker")).toBe(true);
    expect(isClickTool("select")).toBe(false);
  });
});

describe("click tools", () => {
  it("place markers with the next number and a size from the text size", () => {
    const doc = { ...blank(), shapes: [marker("m1", 1), marker("m3", 3)] };
    expect(markerAt({ x: 5, y: 6 }, ctx({ doc }))).toEqual({
      id: "new",
      kind: "marker",
      center: { x: 5, y: 6 },
      number: 4,
      color: "#E53935",
      size: 32,
    });
  });

  it("only count clicks on the shown image", () => {
    expect(insideImage({ x: 0, y: 600 }, blank())).toBe(true);
    expect(insideImage({ x: -1, y: 10 }, blank())).toBe(false);
    const cropped = { ...blank(), crop: { x: 100, y: 100, width: 50, height: 50 } };
    expect(insideImage({ x: 10, y: 10 }, cropped)).toBe(false);
    expect(insideImage({ x: 120, y: 120 }, cropped)).toBe(true);
  });
});
