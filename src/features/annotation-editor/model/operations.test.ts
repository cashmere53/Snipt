import { describe, expect, it } from "vitest";
import type { AnnotationDocument, MarkerShape } from "./document";
import {
  addShape,
  commitText,
  deleteShapes,
  nextMarkerNumber,
  setCrop,
  updateShape,
} from "./operations";
import { arrow, blank, marker, rect, text } from "./testing";

/** Deep-freezes a document so any mutation throws. */
function frozen(doc: AnnotationDocument): AnnotationDocument {
  const freeze = (value: unknown) => {
    if (typeof value === "object" && value !== null) {
      Object.values(value).forEach(freeze);
      Object.freeze(value);
    }
  };
  freeze(doc);
  return doc;
}

const ids = (doc: AnnotationDocument) => doc.shapes.map((shape) => shape.id);

describe("shape operations", () => {
  it("adds, updates and deletes without changing the input", () => {
    const start = frozen(addShape(addShape(blank(), arrow("a")), rect("r")));
    expect(ids(start)).toEqual(["a", "r"]);

    const moved = updateShape(start, "r", { rect: { x: 0, y: 0, width: 10, height: 10 } });
    expect(moved.shapes[1]).toEqual({ ...rect("r"), rect: { x: 0, y: 0, width: 10, height: 10 } });
    expect(start.shapes[1]).toEqual(rect("r"));

    const recolored = updateShape(moved, "a", { color: "#43A047" });
    expect(recolored.shapes[0]).toMatchObject({ id: "a", kind: "arrow", color: "#43A047" });

    expect(ids(deleteShapes(recolored, ["a", "r"]))).toEqual([]);
    expect(ids(start)).toEqual(["a", "r"]);
  });

  it("returns the same document when nothing changes", () => {
    const doc = frozen(addShape(blank(), arrow("a")));
    expect(updateShape(doc, "missing", { color: "#FFFFFF" })).toBe(doc);
    expect(deleteShapes(doc, ["missing"])).toBe(doc);
    expect(deleteShapes(doc, [])).toBe(doc);
  });
});

describe("crop", () => {
  it("is a view of the image that can be removed again", () => {
    const doc = frozen(addShape(blank(), arrow("a")));
    const cropped = setCrop(doc, { x: 10, y: 20, width: 300, height: 200 });
    expect(cropped.crop).toEqual({ x: 10, y: 20, width: 300, height: 200 });
    expect(cropped.shapes).toBe(doc.shapes);

    const uncropped = setCrop(cropped, null);
    expect(uncropped.crop).toBeNull();
    expect(uncropped).toEqual(doc);
    expect(doc.crop).toBeNull();
  });

  it("returns the same document for the same crop", () => {
    const cropped = setCrop(blank(), { x: 10, y: 20, width: 300, height: 200 });
    expect(setCrop(cropped, { x: 10, y: 20, width: 300, height: 200 })).toBe(cropped);
    const doc = blank();
    expect(setCrop(doc, null)).toBe(doc);
  });
});

describe("nextMarkerNumber", () => {
  it("starts at 1", () => {
    expect(nextMarkerNumber(addShape(blank(), arrow("a")))).toBe(1);
  });

  it("keeps the other numbers after a deletion and continues from the largest", () => {
    let doc = blank();
    for (const n of [1, 2, 3]) {
      doc = addShape(doc, marker(`m${n}`, nextMarkerNumber(doc)));
      expect((doc.shapes.at(-1) as MarkerShape).number).toBe(n);
    }

    doc = deleteShapes(doc, ["m2"]);
    expect(doc.shapes.map((s) => (s as MarkerShape).number)).toEqual([1, 3]);
    expect(nextMarkerNumber(doc)).toBe(4);

    doc = deleteShapes(doc, ["m3"]);
    expect(nextMarkerNumber(doc)).toBe(2);

    doc = updateShape<MarkerShape>(doc, "m1", { center: { x: 0, y: 0 } });
    expect((doc.shapes[0] as MarkerShape).number).toBe(1);
  });
});

describe("commitText", () => {
  it("does not create an empty text", () => {
    const doc = frozen(blank());
    expect(commitText(doc, text("t", ""))).toBe(doc);
    expect(commitText(doc, text("t", " \n "))).toBe(doc);
  });

  it("adds a new text and replaces an edited one", () => {
    const added = commitText(blank(), text("t", "1 行目\n2 行目"));
    expect(added.shapes).toEqual([text("t", "1 行目\n2 行目")]);
    const edited = commitText(added, text("t", "直した"));
    expect(edited.shapes).toEqual([text("t", "直した")]);
  });

  it("removes an existing text emptied while editing", () => {
    const doc = commitText(addShape(blank(), arrow("a")), text("t"));
    expect(ids(commitText(doc, text("t", "")))).toEqual(["a"]);
  });
});
