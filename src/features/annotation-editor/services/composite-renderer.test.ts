import { beforeAll, describe, expect, it } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import type { AnnotationDocument, MosaicShape, RectShape } from "../model/document";
import { emptyDocument } from "../model/document";
import { arrow, marker, text } from "../model/testing";
import { drawComposite, renderComposite, renderErrorMessage } from "./composite-renderer";
import { ImageSource } from "./image-source";

const WIDTH = 64;
const HEIGHT = 48;

/** Every pixel differs from its neighbors, so a block of one color cannot be original. */
function original(width = WIDTH, height = HEIGHT): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d")!;
  const image = context.createImageData(width, height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      image.data.set([(x * 4) % 256, (y * 5) % 256, (x * y) % 256, 255], (y * width + x) * 4);
    }
  }
  context.putImageData(image, 0, 0);
  return canvas;
}

const pixels = (canvas: HTMLCanvasElement, x = 0, y = 0, w = canvas.width, h = canvas.height) =>
  canvas.getContext("2d")!.getImageData(x, y, w, h).data;

function doc(overrides: Partial<AnnotationDocument> = {}): AnnotationDocument {
  return { ...emptyDocument({ width: WIDTH, height: HEIGHT }), ...overrides };
}

const outside: RectShape = {
  id: "far",
  kind: "rect",
  rect: { x: 2, y: 2, width: 6, height: 6 },
  color: "#E53935",
  strokeWidth: 2,
};

describe("composite renderer", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);

  it("draws the whole image 1:1 when nothing is cropped", () => {
    const image = original();
    const canvas = drawComposite(doc(), new ImageSource(image));
    expect([canvas.width, canvas.height]).toEqual([WIDTH, HEIGHT]);
    expect([...pixels(canvas)]).toEqual([...pixels(image)]);
  });

  it("outputs the crop's size and leaves shapes outside it out", () => {
    const image = original();
    const crop = { x: 20, y: 16, width: 30, height: 20 };
    const canvas = drawComposite(doc({ crop, shapes: [outside] }), new ImageSource(image));
    expect([canvas.width, canvas.height]).toEqual([30, 20]);
    expect([...pixels(canvas)]).toEqual([...pixels(image, 20, 16, 30, 20)]);
  });

  it("fills mosaics with block colors only, below the other shapes", () => {
    const mosaic: MosaicShape = {
      id: "m",
      kind: "mosaic",
      rect: { x: 16, y: 16, width: 32, height: 16 },
      blockSize: 16,
    };
    const canvas = drawComposite(doc({ shapes: [mosaic] }), new ImageSource(original()));
    for (const left of [16, 32]) {
      const block = pixels(canvas, left, 16, 16, 16);
      const first = [...block.subarray(0, 4)];
      for (let i = 0; i < block.length; i += 4) {
        expect([...block.subarray(i, i + 4)]).toEqual(first);
      }
    }
    // A shape over the mosaic stays on top even when listed first.
    const cover: RectShape = { ...outside, rect: { x: 20, y: 20, width: 8, height: 8 } };
    const covered = drawComposite(doc({ shapes: [cover, mosaic] }), new ImageSource(original()));
    expect([...pixels(covered, 20, 20, 1, 1)]).toEqual([229, 57, 53, 255]);
  });

  it("draws arrows, texts and markers", () => {
    const image = original();
    const shapes = [
      { ...arrow("a"), start: { x: 4, y: 4 }, end: { x: 40, y: 20 } },
      { ...text("t", "A"), position: { x: 2, y: 24 } },
      { ...marker("n", 1), center: { x: 50, y: 30 }, size: 24 },
    ];
    const canvas = drawComposite(doc({ shapes }), new ImageSource(image));
    expect([...pixels(canvas)]).not.toEqual([...pixels(image)]);
    // The marker's circle is filled with its color (beside the number).
    expect([...pixels(canvas, 41, 30, 1, 1)]).toEqual([229, 57, 53, 255]);
  });

  it("encodes PNG and JPEG", async () => {
    const source = new ImageSource(original());
    const png = await renderComposite(doc(), source, "png");
    expect(png.ok && [...png.bytes.subarray(0, 4)]).toEqual([137, 80, 78, 71]);
    expect(png.ok && [png.width, png.height]).toEqual([WIDTH, HEIGHT]);
    const jpeg = await renderComposite(doc(), source, "jpeg");
    expect(jpeg.ok && [...jpeg.bytes.subarray(0, 3)]).toEqual([0xff, 0xd8, 0xff]);
  });

  it("refuses outputs over the canvas limits before drawing", async () => {
    const huge = { width: 40_000, height: 10 };
    const source = new ImageSource({ width: huge.width, height: huge.height } as never);
    const result = await renderComposite({ ...doc(), image: huge }, source, "png");
    expect(result).toEqual({ ok: false, error: { kind: "tooLarge", width: 40_000, height: 10 } });

    const area = await renderComposite(
      { ...doc(), image: { width: 20_000, height: 20_000 } },
      source,
      "png",
    );
    expect(area.ok).toBe(false);
    expect(renderErrorMessage({ kind: "tooLarge", width: 40_000, height: 10 })).toContain(
      "40000×10",
    );
  });
});
