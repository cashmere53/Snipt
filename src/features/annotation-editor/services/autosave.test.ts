import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IpcResult } from "@/shared/ipc/annotation-editor";
import type { AnnotationDocument } from "../model/document";
import { DEFAULT_STYLE } from "../model/style";
import { arrow, blank } from "../model/testing";
import { createEditorStore, currentDoc } from "../store/editor-store";
import { createAutosave } from "./autosave";
import type { RenderResult } from "./composite-renderer";

const PNG = new Uint8Array([137, 80, 78, 71]);

/** A save that stays pending until `resolve` is called. */
interface Pending {
  envelope: Uint8Array;
  resolve(result: IpcResult<null>): void;
}

function setup(options: { compose?: () => Promise<RenderResult> } = {}) {
  const store = createEditorStore({ doc: blank(), style: DEFAULT_STYLE });
  const saves: Pending[] = [];
  const save = vi.fn<(envelope: Uint8Array) => Promise<IpcResult<null>>>(
    (envelope) => new Promise<IpcResult<null>>((resolve) => saves.push({ envelope, resolve })),
  );
  const compose = vi.fn<() => Promise<RenderResult>>(
    options.compose ?? (async () => ({ ok: true as const, bytes: PNG, width: 800, height: 600 })),
  );
  const autosave = createAutosave({ store, compose, save });
  const stop = autosave.start();
  let n = 0;
  const change = () =>
    store.getState().commitDoc({ ...currentDoc(store.getState()), shapes: [arrow(`a${++n}`)] });
  return { store, saves, save, compose, autosave, stop, change };
}

/** The document inside a save envelope. */
function savedDoc(envelope: Uint8Array): AnnotationDocument {
  const length = new DataView(envelope.buffer).getUint32(0, true);
  return JSON.parse(new TextDecoder().decode(envelope.subarray(4, 4 + length)));
}

const ok: IpcResult<null> = { ok: true, value: null };

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("autosave", () => {
  it("saves once, 1 second after the last of several changes", async () => {
    const { saves, store, change } = setup();
    change();
    await vi.advanceTimersByTimeAsync(600);
    change();
    await vi.advanceTimersByTimeAsync(600);
    change();
    await vi.advanceTimersByTimeAsync(999);
    expect(saves).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(1);
    expect(saves).toHaveLength(1);
    expect(savedDoc(saves[0]!.envelope).shapes.map((s) => s.id)).toEqual(["a3"]);
    expect(store.getState().saveStatus).toEqual({ kind: "saving" });

    saves[0]!.resolve(ok);
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getState()).toMatchObject({ savedRevision: 3, saveStatus: { kind: "clean" } });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(saves).toHaveLength(1);
  });

  it("saves again after a save during which the document changed", async () => {
    const { saves, store, change } = setup();
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    // The second timer fired during the first save: only one save runs.
    expect(saves).toHaveLength(1);
    expect(savedDoc(saves[0]!.envelope).shapes.map((s) => s.id)).toEqual(["a1"]);

    saves[0]!.resolve(ok);
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getState()).toMatchObject({ savedRevision: 1, saveStatus: { kind: "dirty" } });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(saves).toHaveLength(2);
    expect(savedDoc(saves[1]!.envelope).shapes.map((s) => s.id)).toEqual(["a2"]);
    saves[1]!.resolve(ok);
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getState().saveStatus).toEqual({ kind: "clean" });
  });

  it("marks a failed save and does not retry it by itself", async () => {
    const { saves, store, change } = setup();
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    saves[0]!.resolve({ ok: false, error: { kind: "library", message: "書き込めません" } });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(store.getState().saveStatus).toEqual({ kind: "failed", message: "書き込めません" });
    expect(saves).toHaveLength(1);

    // The next change tries again.
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(saves).toHaveLength(2);
  });

  it("reports a composite that cannot be made, without sending", async () => {
    const { save, store, change } = setup({
      compose: async () => ({ ok: false, error: { kind: "tooLarge", width: 40_000, height: 10 } }),
    });
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    expect(save).not.toHaveBeenCalled();
    expect(store.getState().saveStatus).toMatchObject({ kind: "failed" });
  });

  it("flushes at once, and sends nothing when everything is saved", async () => {
    const { saves, autosave, change } = setup();
    expect(await autosave.flush()).toEqual({ ok: true });
    expect(saves).toHaveLength(0);

    change();
    const flushed = autosave.flush();
    await vi.advanceTimersByTimeAsync(0);
    expect(saves).toHaveLength(1);
    saves[0]!.resolve(ok);
    expect(await flushed).toEqual({ ok: true });

    // The discarded timer does not save again.
    await vi.advanceTimersByTimeAsync(5_000);
    expect(saves).toHaveLength(1);
    expect(await autosave.flush()).toEqual({ ok: true });
    expect(saves).toHaveLength(1);
  });

  it("waits for the save in progress, then saves what came after it", async () => {
    const { saves, autosave, store, change } = setup();
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    change();
    const flushed = autosave.flush();
    saves[0]!.resolve(ok);
    await vi.advanceTimersByTimeAsync(0);
    expect(saves).toHaveLength(2);
    saves[1]!.resolve(ok);
    expect(await flushed).toEqual({ ok: true });
    expect(store.getState()).toMatchObject({ savedRevision: 2, saveStatus: { kind: "clean" } });
  });

  it("retries a failed save on flush and reports a second failure", async () => {
    const { saves, autosave, change } = setup();
    change();
    await vi.advanceTimersByTimeAsync(1_000);
    saves[0]!.resolve({ ok: false, error: { kind: "io", message: "x" } });
    await vi.advanceTimersByTimeAsync(0);

    const flushed = autosave.flush();
    await vi.advanceTimersByTimeAsync(0);
    saves[1]!.resolve({ ok: false, error: { kind: "io", message: "y" } });
    expect(await flushed).toEqual({ ok: false, message: "y" });
  });

  it("stops following the store", async () => {
    const { saves, stop, change } = setup();
    change();
    stop();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(saves).toHaveLength(0);
  });
});
