import type { InvokeArgs, InvokeOptions } from "@tauri-apps/api/core";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { EditorSessionDto } from "@/shared/ipc/annotation-editor";
import { createEditorIpc, type Invoke, toEditorError } from "./editor-ipc";

interface Call {
  cmd: string;
  args?: InvokeArgs;
  options?: InvokeOptions;
}

/** An invoke that records its calls and answers from `answers` by command. */
function recording(answers: Record<string, unknown> = {}): { invoke: Invoke; calls: Call[] } {
  const calls: Call[] = [];
  const invoke = (async (cmd: string, args?: InvokeArgs, options?: InvokeOptions) => {
    calls.push({ cmd, args, options });
    const answer = answers[cmd];
    if (answer instanceof Error) throw answer;
    return answer ?? null;
  }) as Invoke;
  return { invoke, calls };
}

const session: EditorSessionDto = {
  captureId: "20261022_174150",
  width: 4,
  height: 3,
  capturedAt: "2026-10-22T17:41:50+09:00",
  windowTitle: null,
  annotations: null,
  warning: null,
  unsaved: false,
};

afterEach(() => clearMocks());

describe("editor IPC client", () => {
  it("calls each command with its arguments, raw bodies and headers", async () => {
    const { invoke, calls } = recording({
      editor_open_session: session,
      editor_read_original: new Uint8Array([137, 80, 78, 71]).buffer,
      editor_pick_export_path: { token: "t", format: "jpeg", fileName: "a.jpg" },
    });
    const ipc = createEditorIpc(invoke);
    const body = new Uint8Array([1, 2, 3]);

    expect(await ipc.openSession()).toEqual({ ok: true, value: session });
    expect(await ipc.readOriginal()).toEqual({
      ok: true,
      value: new Uint8Array([137, 80, 78, 71]),
    });
    expect(await ipc.save(body)).toEqual({ ok: true, value: null });
    expect(await ipc.copy(body)).toEqual({ ok: true, value: null });
    expect(await ipc.pickExportPath("Snipt_20261022_174150.png")).toEqual({
      ok: true,
      value: { token: "t", format: "jpeg", fileName: "a.jpg" },
    });
    expect(await ipc.writeExport("t", body)).toEqual({ ok: true, value: null });
    expect(await ipc.flushResult("r", false)).toEqual({ ok: true, value: null });

    expect(calls).toEqual([
      { cmd: "editor_open_session" },
      { cmd: "editor_read_original" },
      { cmd: "editor_save", args: body },
      { cmd: "editor_copy", args: body },
      {
        cmd: "editor_pick_export_path",
        args: { suggestedName: "Snipt_20261022_174150.png" },
      },
      {
        cmd: "editor_write_export",
        args: body,
        options: { headers: { "x-snipt-export-token": "t" } },
      },
      { cmd: "editor_flush_result", args: { requestId: "r", saved: false } },
    ]);
  });

  it("returns a cancelled dialog as null", async () => {
    const ipc = createEditorIpc(recording().invoke);
    expect(await ipc.pickExportPath("x.png")).toEqual({ ok: true, value: null });
  });

  it("turns rejections into failed results", async () => {
    mockIPC((cmd) => {
      if (cmd === "editor_save") throw { kind: "invalidPayload", message: "壊れています" };
      throw "Command editor_copy not allowed by ACL";
    });
    const ipc = createEditorIpc();

    expect(await ipc.save(new Uint8Array())).toEqual({
      ok: false,
      error: { kind: "invalidPayload", message: "壊れています" },
    });
    expect(await ipc.copy(new Uint8Array())).toEqual({
      ok: false,
      error: { kind: "io", message: "Command editor_copy not allowed by ACL" },
    });
  });

  it("fails when the original does not arrive as bytes", async () => {
    const ipc = createEditorIpc(recording({ editor_read_original: "nope" }).invoke);
    const result = await ipc.readOriginal();
    expect(result.ok).toBe(false);
  });
});

describe("toEditorError", () => {
  it("keeps editor errors and wraps anything else as io", () => {
    expect(toEditorError({ kind: "notFound", message: "ありません" })).toEqual({
      kind: "notFound",
      message: "ありません",
    });
    expect(toEditorError({ kind: "made-up", message: "x" })).toEqual({
      kind: "io",
      message: '{"kind":"made-up","message":"x"}',
    });
    expect(toEditorError(new Error("boom"))).toEqual({ kind: "io", message: "boom" });
  });
});
