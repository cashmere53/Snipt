import { describe, expect, it, vi } from "vitest";
import { blank } from "../model/testing";
import type { RenderResult } from "./composite-renderer";
import { copyComposite, exportComposite, type Render, suggestedExportName } from "./export-actions";
import { fail, fakeIpc, ok } from "./testing";

const BYTES = new Uint8Array([1, 2, 3]);
const rendered = vi.fn<Render>(async (): Promise<RenderResult> => ({
  ok: true,
  bytes: BYTES,
  width: 800,
  height: 600,
}));
const tooLarge: Render = async () => ({
  ok: false,
  error: { kind: "tooLarge", width: 40_000, height: 10 },
});

describe("copyComposite", () => {
  it("copies the PNG composite", async () => {
    const ipc = fakeIpc();
    expect(await copyComposite(blank(), rendered, ipc)).toBeNull();
    expect(rendered).toHaveBeenLastCalledWith(blank(), "png");
    expect(ipc.copy).toHaveBeenCalledWith(BYTES);
  });

  it("returns why it failed", async () => {
    const ipc = fakeIpc({ copy: async () => fail("clipboard", "クリップボードを使えません") });
    expect(await copyComposite(blank(), rendered, ipc)).toBe("クリップボードを使えません");
    expect(await copyComposite(blank(), tooLarge, ipc)).toContain("40000×10");
  });
});

describe("exportComposite", () => {
  it("writes the composite in the chosen format with the target's token", async () => {
    const ipc = fakeIpc({
      pickExportPath: async () => ok({ token: "tok", format: "jpeg", fileName: "a.jpg" }),
    });
    expect(await exportComposite(blank(), rendered, ipc, "name.png")).toBeNull();
    expect(rendered).toHaveBeenLastCalledWith(blank(), "jpeg");
    expect(ipc.writeExport).toHaveBeenCalledWith("tok", BYTES);
  });

  it("does nothing when the dialog is cancelled", async () => {
    const ipc = fakeIpc({ pickExportPath: async () => ok(null) });
    rendered.mockClear();
    expect(await exportComposite(blank(), rendered, ipc, "name.png")).toBeNull();
    expect(rendered).not.toHaveBeenCalled();
    expect(ipc.writeExport).not.toHaveBeenCalled();
  });

  it("returns why it failed", async () => {
    const dialog = fakeIpc({ pickExportPath: async () => fail("dialog", "ダイアログ失敗") });
    expect(await exportComposite(blank(), rendered, dialog, "n")).toBe("ダイアログ失敗");
    const write = fakeIpc({ writeExport: async () => fail("io", "書き込めません") });
    expect(await exportComposite(blank(), rendered, write, "n")).toBe("書き込めません");
    expect(await exportComposite(blank(), tooLarge, fakeIpc(), "n")).not.toBeNull();
  });

  it("suggests a name that no capture file uses", () => {
    expect(suggestedExportName("20261022_174150")).toBe("Snipt_20261022_174150_export.png");
  });
});
