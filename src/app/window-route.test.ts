import { describe, expect, it } from "vitest";
import { resolveWindowRoute } from "./window-route";

describe("resolveWindowRoute", () => {
  it("routes fixed labels", () => {
    expect(resolveWindowRoute("settings")).toEqual({ kind: "settings" });
    expect(resolveWindowRoute("history")).toEqual({ kind: "history" });
  });

  it("passes the capture id of an editor window", () => {
    expect(resolveWindowRoute("editor-abc")).toEqual({ kind: "editor", captureId: "abc" });
    expect(resolveWindowRoute("editor-20261022_173512_01")).toEqual({
      kind: "editor",
      captureId: "20261022_173512_01",
    });
  });

  it("passes the monitor index of an overlay window as a number", () => {
    expect(resolveWindowRoute("overlay-1-1")).toEqual({ kind: "overlay", monitorIndex: 1 });
    expect(resolveWindowRoute("overlay-12-0")).toEqual({ kind: "overlay", monitorIndex: 0 });
  });

  it.each([
    "main",
    "editor-",
    "overlay-",
    "overlay-1",
    "overlay-x",
    "overlay--1",
    "overlay-1-",
    "overlay-1-x",
    "Settings",
  ])("treats %s as unknown", (label) => {
    expect(resolveWindowRoute(label)).toEqual({ kind: "unknown", label });
  });
});
