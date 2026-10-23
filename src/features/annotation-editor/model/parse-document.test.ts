import { describe, expect, it } from "vitest";
import type { AnnotationDocument, MosaicShape } from "./document";
import { addShape, setCrop } from "./operations";
import { Invalid, migrateDocument, parseDocument } from "./parse-document";
import { arrow, blank, IMAGE, marker, rect, text } from "./testing";

const mosaic = (id: string): MosaicShape => ({
  id,
  kind: "mosaic",
  rect: { x: 100, y: 100, width: 64, height: 32 },
  blockSize: 16,
});

function full(): AnnotationDocument {
  let doc = setCrop(blank(), { x: 10, y: 10, width: 400, height: 300 });
  for (const shape of [
    arrow("a"),
    rect("r"),
    text("t", "注釈\n2 行目"),
    mosaic("m"),
    marker("k", 1),
  ]) {
    doc = addShape(doc, shape);
  }
  return doc;
}

/** The stored form: JSON text, as capture-library keeps it. */
const stored = (doc: unknown): unknown => JSON.parse(JSON.stringify(doc));

/**
 * The stored form of `full()` with the field at `path` (dot separated, array indexes as
 * numbers) set to `value`, or removed when `value` is `REMOVE`.
 */
const REMOVE = Symbol("remove");
function changed(path: string, value: unknown): unknown {
  const doc = stored(full());
  const keys = path.split(".");
  const last = keys.pop()!;
  let target = doc as Record<string, unknown>;
  for (const key of keys) {
    target = target[key] as Record<string, unknown>;
  }
  if (value === REMOVE) {
    delete target[last];
  } else {
    target[last] = value;
  }
  return doc;
}

describe("parseDocument", () => {
  it("reads back a version 1 document", () => {
    expect(parseDocument(stored(full()), IMAGE)).toEqual({
      ok: true,
      doc: full(),
      migrated: false,
    });
  });

  it("drops unknown fields", () => {
    const raw = stored(full()) as { extra?: boolean; shapes: Record<string, unknown>[] };
    raw.extra = true;
    raw.shapes[0]!.note = "x";
    expect(parseDocument(raw, IMAGE)).toEqual({ ok: true, doc: full(), migrated: false });
  });

  it("rejects newer versions as unsupported", () => {
    expect(parseDocument(changed("schemaVersion", 2), IMAGE)).toEqual({
      ok: false,
      reason: "unsupportedVersion",
    });
  });

  it.each([
    ["not an object", () => "doc"],
    ["no version", () => changed("schemaVersion", REMOVE)],
    ["version 0", () => changed("schemaVersion", 0)],
    ["fractional version", () => changed("schemaVersion", 1.5)],
    ["another image size", () => changed("image.width", 801)],
    ["shapes missing", () => changed("shapes", REMOVE)],
    ["invalid color", () => changed("shapes.0.color", "red")],
    ["short color", () => changed("shapes.1.color", "#fff")],
    ["duplicate ids", () => changed("shapes.1.id", "a")],
    ["empty id", () => changed("shapes.0.id", "")],
    ["unknown kind", () => changed("shapes.0.kind", "blur")],
    ["non-finite number", () => changed("shapes.0.start.x", "10")],
    ["zero stroke", () => changed("shapes.0.strokeWidth", 0)],
    ["empty rect", () => changed("shapes.1.rect.width", 0)],
    ["empty text", () => changed("shapes.2.text", " ")],
    ["unknown text background", () => changed("shapes.2.background", "red")],
    ["mosaic outside the image", () => changed("shapes.3.rect.x", 790)],
    ["fractional mosaic", () => changed("shapes.3.rect.y", 100.5)],
    ["small mosaic block", () => changed("shapes.3.blockSize", 8)],
    ["marker number 0", () => changed("shapes.4.number", 0)],
    ["crop outside the image", () => changed("crop.height", 600)],
    ["crop missing", () => changed("crop", REMOVE)],
  ])("rejects a document with %s as invalid", (_, raw) => {
    expect(parseDocument(raw(), IMAGE)).toEqual({ ok: false, reason: "invalid" });
  });

  it("passes a current document through the (empty) migrations unchanged", () => {
    const result = parseDocument(stored(full()), IMAGE, {
      1: () => {
        throw new Error("version 1 is current; nothing to migrate");
      },
    });
    expect(result).toEqual({ ok: true, doc: full(), migrated: false });
  });
});

describe("migrateDocument", () => {
  const migrations = {
    1: (doc: Record<string, unknown>) => ({ ...doc, schemaVersion: 2, steps: ["1→2"] }),
    2: (doc: Record<string, unknown>) => ({
      ...doc,
      schemaVersion: 3,
      steps: [...(doc.steps as string[]), "2→3"],
    }),
  };

  it("applies one step per version, in order", () => {
    expect(migrateDocument({ schemaVersion: 1 }, 1, 3, migrations)).toEqual({
      schemaVersion: 3,
      steps: ["1→2", "2→3"],
    });
    expect(migrateDocument({ schemaVersion: 2, steps: ["0→2"] }, 2, 3, migrations)).toEqual({
      schemaVersion: 3,
      steps: ["0→2", "2→3"],
    });
  });

  it("fails when a step is missing or returns something else than an object", () => {
    expect(() => migrateDocument({}, 1, 4, migrations)).toThrow(Invalid);
    expect(() => migrateDocument({}, 1, 2, { 1: () => null })).toThrow(Invalid);
  });
});
