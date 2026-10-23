// The composite image at the original resolution, independent of the view: the output area
// (the crop, or the whole image) with the original image, the mosaics and the other shapes,
// drawn by an offscreen Konva stage at scale 1.
import Konva from "konva";
import type { ExportFormat } from "@/shared/ipc/annotation-editor";
import { arrowAttrs, markerAttrs, mosaicAttrs, rectAttrs, textAttrs } from "../canvas/shape-attrs";
import type { AnnotationDocument, Rect, Shape } from "../model/document";
import { pixelate } from "../model/mosaic";
import type { ImageSource } from "./image-source";

/** Canvas limits of Chromium (WebView2): each side and the area. */
export const MAX_CANVAS_SIDE = 32_767;
export const MAX_CANVAS_AREA = 268_435_456;
export const JPEG_QUALITY = 0.92;

export type RenderError =
  | { kind: "tooLarge"; width: number; height: number }
  | { kind: "encodeFailed" };

export type RenderResult =
  | { ok: true; bytes: Uint8Array; width: number; height: number }
  | { ok: false; error: RenderError };

export function renderErrorMessage(error: RenderError): string {
  switch (error.kind) {
    case "tooLarge":
      return `画像が大きすぎるため作成できません(${error.width}×${error.height} px。1 辺 32,767 px まで、合計 268,435,456 px まで)`;
    case "encodeFailed":
      return "画像を作成できませんでした";
  }
}

export function outputArea(doc: AnnotationDocument): Rect {
  return doc.crop ?? { x: 0, y: 0, ...doc.image };
}

function tooLarge(area: Rect): boolean {
  return (
    area.width > MAX_CANVAS_SIDE ||
    area.height > MAX_CANVAS_SIDE ||
    area.width * area.height > MAX_CANVAS_AREA
  );
}

function blockImage(source: ImageSource, shape: Extract<Shape, { kind: "mosaic" }>) {
  const blocks = pixelate(source.regionPixels(shape.rect), shape.blockSize);
  const canvas = document.createElement("canvas");
  canvas.width = blocks.width;
  canvas.height = blocks.height;
  const context = canvas.getContext("2d")!;
  const image = context.createImageData(blocks.width, blocks.height);
  image.data.set(blocks.data);
  context.putImageData(image, 0, 0);
  return canvas;
}

/** The Konva node of a shape other than a mosaic, from the shared `shape-attrs`. */
export function shapeNode(shape: Exclude<Shape, { kind: "mosaic" }>): Konva.Node {
  switch (shape.kind) {
    case "arrow":
      return new Konva.Arrow(arrowAttrs(shape));
    case "rect":
      return new Konva.Rect(rectAttrs(shape));
    case "text": {
      const attrs = textAttrs(shape);
      const label = new Konva.Label(attrs.label);
      label.add(new Konva.Tag(attrs.tag));
      label.add(new Konva.Text(attrs.text));
      return label;
    }
    case "marker": {
      const attrs = markerAttrs(shape);
      const group = new Konva.Group(attrs.group);
      group.add(new Konva.Circle(attrs.circle));
      group.add(new Konva.Text(attrs.label));
      return group;
    }
  }
}

/**
 * Draws the output area into a new canvas of its size. The caller checks the size limits.
 * Mosaics are computed from the original pixels and drawn below the other shapes.
 */
export function drawComposite(doc: AnnotationDocument, source: ImageSource): HTMLCanvasElement {
  const area = outputArea(doc);
  const stage = new Konva.Stage({
    container: document.createElement("div"),
    width: area.width,
    height: area.height,
  });
  try {
    const layer = new Konva.Layer({ x: -area.x, y: -area.y, imageSmoothingEnabled: false });
    stage.add(layer);
    layer.add(new Konva.Image({ image: source.image, x: 0, y: 0 }));
    for (const shape of doc.shapes) {
      if (shape.kind === "mosaic") {
        layer.add(new Konva.Image({ ...mosaicAttrs(shape), image: blockImage(source, shape) }));
      }
    }
    for (const shape of doc.shapes) {
      if (shape.kind !== "mosaic") layer.add(shapeNode(shape) as Konva.Shape | Konva.Group);
    }
    return stage.toCanvas({ x: 0, y: 0, width: area.width, height: area.height, pixelRatio: 1 });
  } finally {
    stage.destroy();
  }
}

function encode(canvas: HTMLCanvasElement, format: ExportFormat): Promise<Blob | null> {
  const type = format === "jpeg" ? "image/jpeg" : "image/png";
  return new Promise((resolve) => {
    try {
      canvas.toBlob(resolve, type, format === "jpeg" ? JPEG_QUALITY : undefined);
    } catch {
      resolve(null);
    }
  });
}

export async function renderComposite(
  doc: AnnotationDocument,
  source: ImageSource,
  format: ExportFormat,
): Promise<RenderResult> {
  const area = outputArea(doc);
  if (tooLarge(area)) {
    return { ok: false, error: { kind: "tooLarge", width: area.width, height: area.height } };
  }
  let blob: Blob | null;
  try {
    blob = await encode(drawComposite(doc, source), format);
  } catch {
    blob = null;
  }
  if (blob === null) return { ok: false, error: { kind: "encodeFailed" } };
  return {
    ok: true,
    bytes: new Uint8Array(await blob.arrayBuffer()),
    width: area.width,
    height: area.height,
  };
}
