import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockConvertFileSrc, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  type CaptureId,
  type CaptureItem,
  copyCapture,
  deleteCaptures,
  listCaptures,
  onLibraryEvent,
  openInEditor,
  pruneMissing,
  revealCapture,
  thumbnailSrc,
} from "./capture-library";

const id = (s: string) => s as CaptureId;

const item: CaptureItem = {
  id: id("20261022_174150"),
  capturedAt: "2026-10-22T17:41:50.123+09:00",
  mode: "region",
  width: 1920,
  height: 1080,
  windowTitle: null,
  originalPath: "C:\\shots\\Snipt_20261022_174150.png",
  annotatedPath: null,
  hasAnnotations: false,
  thumbnailPath: "C:\\shots\\.snipt\\thumbs\\20261022_174150.png",
  thumbnailVersion: 1761122510123,
};

afterEach(() => clearMocks());

describe("thumbnailSrc", () => {
  it("is null until the thumbnail exists", () => {
    expect(thumbnailSrc({ ...item, thumbnailPath: null })).toBeNull();
  });

  it("converts the path to an asset URL with the version query", () => {
    mockConvertFileSrc("windows");
    const src = thumbnailSrc(item);
    expect(src).toMatch(/^http:\/\/asset\.localhost\//);
    expect(src).toContain(encodeURIComponent("20261022_174150.png"));
    expect(src).toMatch(/\?v=1761122510123$/);
  });
});

describe("capture-library IPC wrappers", () => {
  it("call the library commands with the documented names and arguments", async () => {
    const calls: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      switch (cmd) {
        case "library_list":
          return [item];
        case "library_delete":
          return { removed: [item.id], failed: [] };
        case "library_prune_missing":
          return [item.id];
        default:
          return null;
      }
    });

    await expect(listCaptures()).resolves.toEqual([item]);
    await openInEditor(item.id);
    await copyCapture(item.id);
    await revealCapture(item.id);
    await expect(deleteCaptures([item.id])).resolves.toEqual({ removed: [item.id], failed: [] });
    await expect(pruneMissing([item.id])).resolves.toEqual([item.id]);

    expect(calls).toEqual([
      { cmd: "library_list", args: {} },
      { cmd: "library_open_in_editor", args: { id: item.id } },
      { cmd: "library_copy", args: { id: item.id } },
      { cmd: "library_reveal", args: { id: item.id } },
      { cmd: "library_delete", args: { ids: [item.id] } },
      { cmd: "library_prune_missing", args: { ids: [item.id] } },
    ]);
  });

  it("delivers typed event payloads", async () => {
    mockIPC(() => null, { shouldMockEvents: true });
    const received: unknown[] = [];
    const unlisten = await onLibraryEvent("library://items-removed", (payload) =>
      received.push(payload),
    );

    await emit("library://items-removed", { ids: [item.id], reason: "deleted" });
    unlisten();
    await emit("library://items-removed", { ids: [], reason: "deleted" });

    expect(received).toEqual([{ ids: [item.id], reason: "deleted" }]);
  });
});
