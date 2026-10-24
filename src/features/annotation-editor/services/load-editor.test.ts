import { describe, expect, it } from "vitest";
import { CURRENT_SCHEMA_VERSION } from "../model/document";
import { rect } from "../model/testing";
import {
  NEWER_ANNOTATIONS,
  UNREADABLE_ANNOTATIONS,
  UNREADABLE_IMAGE,
  loadEditor,
} from "./load-editor";
import { fail, fakeIpc, ok, sessionDto } from "./testing";

const image = { width: 400, height: 300 } as never;
const decode = async () => image;
const broken = async () => Promise.reject(new Error("bad png"));

const stored = (shapes: unknown[]) => ({
  schemaVersion: CURRENT_SCHEMA_VERSION,
  image: { width: 400, height: 300 },
  crop: null,
  shapes,
});

describe("loadEditor", () => {
  it("opens the original with its stored annotations", async () => {
    const ipc = fakeIpc({
      openSession: async () => ok(sessionDto({ annotations: stored([rect("r")]) })),
    });
    const loaded = await loadEditor(ipc, decode);
    expect(loaded).toMatchObject({ ok: true, warnings: [] });
    if (!loaded.ok) return;
    expect(loaded.doc.shapes.map((s) => s.id)).toEqual(["r"]);
    expect(loaded.doc.image).toEqual({ width: 400, height: 300 });
    expect(loaded.source.image).toBe(image);
  });

  it("opens without annotations when there are none", async () => {
    const loaded = await loadEditor(fakeIpc(), decode);
    expect(loaded).toMatchObject({ ok: true, warnings: [], doc: { shapes: [], crop: null } });
  });

  it.each([
    [{ schemaVersion: 1, image: "?" }, UNREADABLE_ANNOTATIONS],
    [{ ...stored([]), schemaVersion: CURRENT_SCHEMA_VERSION + 1 }, NEWER_ANNOTATIONS],
  ])("opens the original alone with a warning for %j", async (annotations, warning) => {
    const ipc = fakeIpc({ openSession: async () => ok(sessionDto({ annotations })) });
    const loaded = await loadEditor(ipc, decode);
    expect(loaded).toMatchObject({ ok: true, warnings: [warning], doc: { shapes: [] } });
  });

  it("passes the session's warning on", async () => {
    const ipc = fakeIpc({
      openSession: async () => ok(sessionDto({ warning: "まだ保存されていません" })),
    });
    expect(await loadEditor(ipc, decode)).toMatchObject({
      ok: true,
      warnings: ["まだ保存されていません"],
    });
  });

  it("fails when the session, the original or its decoding fails", async () => {
    const noSession = fakeIpc({ openSession: async () => fail("notFound", "見つかりません") });
    expect(await loadEditor(noSession, decode)).toEqual({ ok: false, message: "見つかりません" });

    const noOriginal = fakeIpc({ readOriginal: async () => fail("io", "読めません") });
    expect(await loadEditor(noOriginal, decode)).toEqual({ ok: false, message: "読めません" });

    expect(await loadEditor(fakeIpc(), broken)).toEqual({ ok: false, message: UNREADABLE_IMAGE });
  });
});
