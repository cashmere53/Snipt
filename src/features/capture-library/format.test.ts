import { describe, expect, it } from "vitest";
import { formatCapturedAt, formatSize } from "./format";

describe("formatCapturedAt", () => {
  it("shows the capture's own clock time", () => {
    expect(formatCapturedAt("2026-10-22T17:41:50.123+09:00")).toBe("2026/10/22 17:41:50");
    expect(formatCapturedAt("2026-01-02T03:04:05Z")).toBe("2026/01/02 03:04:05");
  });

  it("leaves unexpected text as is", () => {
    expect(formatCapturedAt("yesterday")).toBe("yesterday");
  });
});

describe("formatSize", () => {
  it("joins width and height", () => {
    expect(formatSize(1920, 1080)).toBe("1920 × 1080");
  });
});
