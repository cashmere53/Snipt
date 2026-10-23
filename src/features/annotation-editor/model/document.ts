// The annotation document: the persisted schema of a capture's annotations. capture-library
// only reads `schemaVersion`; this file is the source of truth for the rest.

export const CURRENT_SCHEMA_VERSION = 1 as const;

/** Coordinates and sizes are physical pixels of the original image (before cropping). */
export interface Point {
  x: number;
  y: number;
}

/** `width` and `height` are positive. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ImageSize {
  width: number;
  height: number;
}

/** `#RRGGBB`; checked by `parse-document` and `style`. */
export type HexColor = `#${string}`;

/** `crypto.randomUUID()`. */
export type ShapeId = string;

interface ShapeBase {
  id: ShapeId;
}

export interface ArrowShape extends ShapeBase {
  kind: "arrow";
  start: Point;
  /** The arrowhead is at `end`. */
  end: Point;
  color: HexColor;
  strokeWidth: number;
}

/** An outline without fill. */
export interface RectShape extends ShapeBase {
  kind: "rect";
  rect: Rect;
  color: HexColor;
  strokeWidth: number;
}

/** With `"color"`, the background is `color` and the text white or black by luminance. */
export type TextBackground = "none" | "white" | "color";

export const TEXT_BACKGROUNDS: readonly TextBackground[] = ["none", "white", "color"];

export interface TextShape extends ShapeBase {
  kind: "text";
  /** Top left. */
  position: Point;
  /** Not empty; lines are separated by `\n`. */
  text: string;
  color: HexColor;
  fontSize: number;
  background: TextBackground;
}

export interface MosaicShape extends ShapeBase {
  kind: "mosaic";
  /** Integers, inside the image. */
  rect: Rect;
  /** Integer, at least `MIN_MOSAIC_BLOCK`. */
  blockSize: number;
}

export interface MarkerShape extends ShapeBase {
  kind: "marker";
  center: Point;
  /** Integer from 1; never renumbered once placed. */
  number: number;
  color: HexColor;
  /** Diameter in pixels. */
  size: number;
}

export type Shape = ArrowShape | RectShape | TextShape | MosaicShape | MarkerShape;
export type ShapeKind = Shape["kind"];

export const SHAPE_KINDS: readonly ShapeKind[] = ["arrow", "rect", "text", "mosaic", "marker"];

export interface AnnotationDocumentV1 {
  schemaVersion: 1;
  /** The original image size, to check the document belongs to it. */
  image: ImageSize;
  /** `null` = not cropped. Integers, inside the image. */
  crop: Rect | null;
  /** Drawing order, last on top. Mosaics are always drawn below the other shapes. */
  shapes: readonly Shape[];
}

export type AnnotationDocument = AnnotationDocumentV1;

export function emptyDocument(image: ImageSize): AnnotationDocument {
  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    image: { width: image.width, height: image.height },
    crop: null,
    shapes: [],
  };
}

const HEX_COLOR = /^#[0-9a-f]{6}$/i;

export function isHexColor(value: unknown): value is HexColor {
  return typeof value === "string" && HEX_COLOR.test(value);
}
