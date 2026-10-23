import { describe, expect, it } from "vitest";
import { DEFAULT_STYLE, FONT_SIZES, PALETTE, STROKE_WIDTHS, validateStyle } from "./style";

describe("validateStyle", () => {
  it("keeps a valid style", () => {
    const style = { color: "#1e88e5", strokeWidth: 8, fontSize: 48, textBackground: "color" };
    expect(validateStyle(style)).toEqual(style);
  });

  it("replaces invalid colors, out-of-range numbers and missing fields with the defaults", () => {
    expect(validateStyle({ color: "red", strokeWidth: 5, fontSize: 1000 })).toEqual(DEFAULT_STYLE);
    expect(validateStyle({ color: "#12345", textBackground: "pink" })).toEqual(DEFAULT_STYLE);
    expect(validateStyle({ color: "#FFFFFF", strokeWidth: "4" })).toEqual({
      ...DEFAULT_STYLE,
      color: "#FFFFFF",
    });
  });

  it("falls back entirely for something that is not a style", () => {
    for (const raw of [null, undefined, 42, "style", []]) {
      expect(validateStyle(raw)).toEqual(DEFAULT_STYLE);
    }
  });

  it("offers eight colors and default values that are among the choices", () => {
    expect(PALETTE).toHaveLength(8);
    expect(PALETTE).toContain(DEFAULT_STYLE.color);
    expect(STROKE_WIDTHS).toContain(DEFAULT_STYLE.strokeWidth);
    expect(FONT_SIZES).toContain(DEFAULT_STYLE.fontSize);
  });
});
