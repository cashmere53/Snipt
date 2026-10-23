// Shape and document fixtures for the editor's tests.
import {
  type AnnotationDocument,
  type ArrowShape,
  emptyDocument,
  type MarkerShape,
  type RectShape,
  type TextShape,
} from "./document";

export const IMAGE = { width: 800, height: 600 };

export const blank = (): AnnotationDocument => emptyDocument(IMAGE);

export const arrow = (id: string): ArrowShape => ({
  id,
  kind: "arrow",
  start: { x: 10, y: 10 },
  end: { x: 100, y: 50 },
  color: "#E53935",
  strokeWidth: 4,
});

export const rect = (id: string): RectShape => ({
  id,
  kind: "rect",
  rect: { x: 20, y: 30, width: 200, height: 100 },
  color: "#1E88E5",
  strokeWidth: 2,
});

export const text = (id: string, value = "確認してください"): TextShape => ({
  id,
  kind: "text",
  position: { x: 40, y: 40 },
  text: value,
  color: "#212121",
  fontSize: 24,
  background: "white",
});

export const marker = (id: string, number: number): MarkerShape => ({
  id,
  kind: "marker",
  center: { x: 300, y: 200 },
  number,
  color: "#E53935",
  size: 32,
});
