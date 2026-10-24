import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_STYLE } from "../model/style";
import { blank } from "../model/testing";
import type { SaveResult } from "../services/autosave";
import { fakePort } from "../services/testing";
import { createEditorStore, currentDoc } from "../store/editor-store";
import { useCloseGuard } from "./use-close-guard";

function setup(results: SaveResult[] = [{ ok: true }]) {
  const store = createEditorStore({ doc: blank(), style: DEFAULT_STYLE });
  const flush = vi.fn<() => Promise<SaveResult>>(async () => results.shift() ?? { ok: true });
  const flushResult = vi.fn<(requestId: string, saved: boolean) => Promise<unknown>>(
    async () => null,
  );
  const window = fakePort();
  const hook = renderHook(() => useCloseGuard({ store, flush, port: window.port, flushResult }));
  return { store, flush, flushResult, window, hook };
}

describe("useCloseGuard", () => {
  it("saves before the user's close, the text being typed included, then closes", async () => {
    const f = setup();
    await waitFor(() => expect(f.window.listening()).toBe(true));
    act(() => {
      f.store.getState().setTool("text");
      f.store.getState().beginTextEdit({ position: { x: 10, y: 10 } });
      f.store.getState().updateTextDraft("メモ");
    });
    expect(f.window.userClose()).toBe(true);
    await waitFor(() => expect(f.window.port.destroy).toHaveBeenCalledOnce());
    expect(f.flush).toHaveBeenCalledOnce();
    expect(currentDoc(f.store.getState()).shapes).toMatchObject([{ kind: "text", text: "メモ" }]);
    expect(f.hook.result.current.failure).toBeNull();
  });

  it("asks when the save fails: retry closes once it saves, discard closes", async () => {
    const f = setup([{ ok: false, message: "ディスクがいっぱいです" }]);
    await waitFor(() => expect(f.window.listening()).toBe(true));
    f.window.userClose();
    await waitFor(() => expect(f.hook.result.current.failure).toBe("ディスクがいっぱいです"));
    expect(f.window.port.destroy).not.toHaveBeenCalled();

    act(() => f.hook.result.current.retry());
    await waitFor(() => expect(f.window.port.destroy).toHaveBeenCalledOnce());
    expect(f.flush).toHaveBeenCalledTimes(2);
    expect(f.hook.result.current.failure).toBeNull();

    const g = setup([{ ok: false, message: "失敗" }]);
    await waitFor(() => expect(g.window.listening()).toBe(true));
    g.window.userClose();
    await waitFor(() => expect(g.hook.result.current.failure).toBe("失敗"));
    act(() => g.hook.result.current.discard());
    await waitFor(() => expect(g.window.port.destroy).toHaveBeenCalledOnce());
    expect(g.flush).toHaveBeenCalledOnce();
  });

  it("does not save twice for a second close while saving", async () => {
    const f = setup();
    await waitFor(() => expect(f.window.listening()).toBe(true));
    f.window.userClose();
    expect(f.window.userClose()).toBe(true);
    await waitFor(() => expect(f.window.port.destroy).toHaveBeenCalledOnce());
    expect(f.flush).toHaveBeenCalledOnce();
  });

  it("closes on Rust's request even when the save fails, without asking", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const f = setup([{ ok: false, message: "失敗" }]);
    await waitFor(() => expect(f.window.listening()).toBe(true));
    f.window.closeRequest();
    await waitFor(() => expect(f.window.port.destroy).toHaveBeenCalledOnce());
    expect(f.hook.result.current.failure).toBeNull();
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });

  it("answers the flush request before quitting and keeps the window", async () => {
    const f = setup([{ ok: true }, { ok: false, message: "失敗" }]);
    await waitFor(() => expect(f.window.listening()).toBe(true));
    f.window.flushRequest("r1");
    await waitFor(() => expect(f.flushResult).toHaveBeenCalledWith("r1", true));
    f.window.flushRequest("r2");
    await waitFor(() => expect(f.flushResult).toHaveBeenCalledWith("r2", false));
    expect(f.window.port.destroy).not.toHaveBeenCalled();
    expect(f.hook.result.current.failure).toBeNull();
  });

  it("stops listening when unmounted", async () => {
    const f = setup();
    await waitFor(() => expect(f.window.listening()).toBe(true));
    f.hook.unmount();
    await waitFor(() => expect(f.window.listening()).toBe(false));
  });
});
