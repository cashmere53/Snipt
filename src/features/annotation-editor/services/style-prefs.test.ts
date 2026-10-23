import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_STYLE, type StyleSettings } from "../model/style";
import { loadStyle, saveStyle, STYLE_PREFS_KEY } from "./style-prefs";

const style: StyleSettings = {
  color: "#1E88E5",
  strokeWidth: 8,
  fontSize: 48,
  textBackground: "color",
};

afterEach(() => localStorage.clear());

describe("style-prefs", () => {
  it("returns the defaults when nothing is stored", () => {
    expect(loadStyle()).toEqual(DEFAULT_STYLE);
  });

  it("returns the saved style on the next load", () => {
    saveStyle(style);
    expect(loadStyle()).toEqual(style);
  });

  it("replaces broken or invalid stored values with the defaults", () => {
    localStorage.setItem(STYLE_PREFS_KEY, "{not json");
    expect(loadStyle()).toEqual(DEFAULT_STYLE);

    localStorage.setItem(STYLE_PREFS_KEY, JSON.stringify({ ...style, strokeWidth: 5 }));
    expect(loadStyle()).toEqual({ ...style, strokeWidth: DEFAULT_STYLE.strokeWidth });
  });

  it("falls back to the defaults when storage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    expect(() => saveStyle(style)).not.toThrow();
    expect(loadStyle()).toEqual(DEFAULT_STYLE);
  });
});
