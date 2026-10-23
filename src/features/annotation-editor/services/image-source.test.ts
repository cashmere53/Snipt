import { beforeAll, describe, expect, it, vi } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import { decodePng, ImageSource } from "./image-source";

/** A 6×4 image whose every pixel has a different color. */
function gradient(): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = 6;
  canvas.height = 4;
  const context = canvas.getContext("2d")!;
  const image = context.createImageData(6, 4);
  for (let i = 0; i < 24; i++) {
    image.data.set([i * 10, 255 - i * 10, i, 255], i * 4);
  }
  context.putImageData(image, 0, 0);
  return canvas;
}

describe("ImageSource", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);

  it("returns the original pixels of a region", () => {
    const image = gradient();
    const source = new ImageSource(image);
    expect([source.width, source.height]).toEqual([6, 4]);

    const region = source.regionPixels({ x: 2, y: 1, width: 3, height: 2 });
    const expected = image.getContext("2d")!.getImageData(2, 1, 3, 2);
    expect([region.width, region.height]).toEqual([3, 2]);
    expect([...region.data]).toEqual([...expected.data]);
  });

  it("expands the image only once", () => {
    const source = new ImageSource(gradient());
    const createElement = vi.spyOn(document, "createElement");

    source.regionPixels({ x: 0, y: 0, width: 6, height: 4 });
    source.regionPixels({ x: 5, y: 3, width: 1, height: 1 });

    const canvases = createElement.mock.calls.filter(([tag]) => tag === "canvas");
    expect(canvases).toHaveLength(1);
  });
});

describe("decodePng", () => {
  it("decodes the bytes as a PNG blob", async () => {
    const bitmap = { width: 1, height: 1 } as ImageBitmap;
    const create = vi.fn<(blob: Blob) => Promise<ImageBitmap>>().mockResolvedValue(bitmap);
    vi.stubGlobal("createImageBitmap", create);
    try {
      expect(await decodePng(new Uint8Array([137, 80, 78, 71]))).toBe(bitmap);
      const blob = create.mock.calls[0]![0];
      expect(blob.type).toBe("image/png");
      expect(new Uint8Array(await blob.arrayBuffer())).toEqual(new Uint8Array([137, 80, 78, 71]));
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
