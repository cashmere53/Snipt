import { describe, expect, it } from "vitest";
import type { MosaicShape } from "../model/document";
import { arrow, marker, rect, text } from "../model/testing";
import {
  arrowAttrs,
  contrastingTextColor,
  luminance,
  markerAttrs,
  mosaicAttrs,
  rectAttrs,
  textAttrs,
} from "./shape-attrs";

describe("shape-attrs", () => {
  it("draws arrows with the head at the end", () => {
    const attrs = arrowAttrs({ ...arrow("a"), strokeWidth: 6 });
    expect(attrs.points).toEqual([10, 10, 100, 50]);
    expect(attrs.pointerAtBeginning).toBe(false);
    expect(attrs.pointerAtEnding).toBe(true);
    expect([attrs.stroke, attrs.fill, attrs.strokeWidth]).toEqual(["#E53935", "#E53935", 6]);
    expect([attrs.pointerLength, attrs.pointerWidth]).toEqual([18, 18]);
    // Thin arrows keep a visible head.
    expect(arrowAttrs({ ...arrow("a"), strokeWidth: 2 }).pointerLength).toBe(10);
  });

  it("draws rectangles as outlines", () => {
    expect(rectAttrs(rect("r"))).toMatchObject({
      x: 20,
      y: 30,
      width: 200,
      height: 100,
      stroke: "#1E88E5",
      strokeWidth: 2,
      fillEnabled: false,
    });
  });

  it("gives texts no, a white or a color background", () => {
    const none = textAttrs({ ...text("t"), background: "none", color: "#E53935" });
    expect(none.label).toEqual({ x: 40, y: 40 });
    expect(none.tag.fill).toBeUndefined();
    expect(none.text).toMatchObject({
      text: "確認してください",
      fontSize: 24,
      fill: "#E53935",
      padding: 6,
    });

    const white = textAttrs({ ...text("t"), background: "white", color: "#E53935" });
    expect([white.tag.fill, white.text.fill]).toEqual(["#FFFFFF", "#E53935"]);

    const onYellow = textAttrs({ ...text("t"), background: "color", color: "#FDD835" });
    expect([onYellow.tag.fill, onYellow.text.fill]).toEqual(["#FDD835", "#000000"]);
    const onBlue = textAttrs({ ...text("t"), background: "color", color: "#1E88E5" });
    expect([onBlue.tag.fill, onBlue.text.fill]).toEqual(["#1E88E5", "#FFFFFF"]);
  });

  it("keeps the lines of a text", () => {
    expect(textAttrs(text("t", "一行目\n二行目")).text.text).toBe("一行目\n二行目");
  });

  it("draws markers as numbered circles", () => {
    const attrs = markerAttrs({ ...marker("m", 7), size: 32 });
    expect(attrs.group).toEqual({ x: 300, y: 200 });
    expect(attrs.circle).toMatchObject({ x: 0, y: 0, radius: 16, fill: "#E53935" });
    expect(attrs.label).toMatchObject({
      text: "7",
      x: -16,
      y: -16,
      width: 32,
      height: 32,
      align: "center",
      verticalAlign: "middle",
      fill: "#FFFFFF",
    });
    expect(markerAttrs({ ...marker("m", 1), color: "#FFFFFF" }).label.fill).toBe("#000000");
  });

  it("places mosaics on their rectangle", () => {
    const mosaic: MosaicShape = {
      id: "b",
      kind: "mosaic",
      rect: { x: 16, y: 32, width: 64, height: 48 },
      blockSize: 16,
    };
    expect(mosaicAttrs(mosaic)).toEqual({ x: 16, y: 32, width: 64, height: 48 });
  });
});

describe("contrastingTextColor", () => {
  it("picks black on light colors and white on dark or saturated ones", () => {
    expect(luminance("#FFFFFF")).toBeCloseTo(1);
    expect(luminance("#000000")).toBe(0);
    const cases: [string, string][] = [
      ["#E53935", "#FFFFFF"],
      ["#FB8C00", "#FFFFFF"],
      ["#FDD835", "#000000"],
      ["#43A047", "#FFFFFF"],
      ["#1E88E5", "#FFFFFF"],
      ["#8E24AA", "#FFFFFF"],
      ["#212121", "#FFFFFF"],
      ["#FFFFFF", "#000000"],
    ];
    expect(cases.map(([background]) => contrastingTextColor(background as `#${string}`))).toEqual(
      cases.map(([, expected]) => expected),
    );
  });
});
