// Edits of an annotation document. Pure: the input is never changed, and an edit that changes
// nothing returns the same document (so the history does not record it).
import type { AnnotationDocument, Rect, Shape, ShapeId, TextShape } from "./document";

/** Fields of a shape that may change; its id and kind stay. */
export type ShapePatch<S extends Shape = Shape> = Partial<Omit<S, "id" | "kind">>;

export function addShape(doc: AnnotationDocument, shape: Shape): AnnotationDocument {
  return { ...doc, shapes: [...doc.shapes, shape] };
}

export function updateShape<S extends Shape>(
  doc: AnnotationDocument,
  id: ShapeId,
  patch: ShapePatch<S>,
): AnnotationDocument {
  const index = doc.shapes.findIndex((shape) => shape.id === id);
  if (index < 0) {
    return doc;
  }
  const shapes = [...doc.shapes];
  shapes[index] = { ...shapes[index]!, ...patch } as Shape;
  return { ...doc, shapes };
}

export function deleteShapes(doc: AnnotationDocument, ids: readonly ShapeId[]): AnnotationDocument {
  const doomed = new Set(ids);
  const shapes = doc.shapes.filter((shape) => !doomed.has(shape.id));
  return shapes.length === doc.shapes.length ? doc : { ...doc, shapes };
}

const sameRect = (a: Rect | null, b: Rect | null) =>
  a === b ||
  (a !== null &&
    b !== null &&
    a.x === b.x &&
    a.y === b.y &&
    a.width === b.width &&
    a.height === b.height);

/** `null` removes the crop, showing the whole image again. */
export function setCrop(doc: AnnotationDocument, crop: Rect | null): AnnotationDocument {
  return sameRect(doc.crop, crop) ? doc : { ...doc, crop: crop === null ? null : { ...crop } };
}

/** One more than the largest marker number, or 1; deleting markers never renumbers others. */
export function nextMarkerNumber(doc: AnnotationDocument): number {
  let max = 0;
  for (const shape of doc.shapes) {
    if (shape.kind === "marker" && shape.number > max) {
      max = shape.number;
    }
  }
  return max + 1;
}

/**
 * Commits an edited text: an empty (or blank) text creates nothing and removes an existing
 * one; otherwise the text is added, or replaces the shape with the same id.
 */
export function commitText(doc: AnnotationDocument, text: TextShape): AnnotationDocument {
  const exists = doc.shapes.some((shape) => shape.id === text.id);
  if (text.text.trim() === "") {
    return exists ? deleteShapes(doc, [text.id]) : doc;
  }
  return exists ? updateShape<TextShape>(doc, text.id, text) : addShape(doc, text);
}
