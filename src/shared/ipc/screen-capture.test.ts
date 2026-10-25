import { describe, expect, it } from "vitest";
import { OVERLAY_COMMANDS, OVERLAY_ERROR_KINDS } from "./screen-capture";

describe("screen-capture IPC contract", () => {
  it("names the overlay commands as build.rs lists them", () => {
    expect(Object.values(OVERLAY_COMMANDS)).toEqual([
      "capture_overlay_ready",
      "capture_overlay_shown",
      "capture_overlay_magnifier",
      "capture_overlay_pointer",
      "capture_overlay_commit_region",
      "capture_overlay_commit_window",
      "capture_overlay_cancel",
    ]);
  });

  it("has the error kinds of OverlayCommandError", () => {
    expect(OVERLAY_ERROR_KINDS).toEqual([
      "tooSmall",
      "noWindowHighlighted",
      "staleSession",
      "topologyChanged",
      "outOfRange",
      "notAnOverlay",
    ]);
  });
});
