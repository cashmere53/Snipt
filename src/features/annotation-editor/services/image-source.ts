// The original image: decoded once from the PNG bytes of `editor_read_original`, drawn by the
// canvas, and expanded to pixels once (on first use) for the mosaics.
import type { ImageSize, Rect } from "../model/document";
import type { Pixels } from "../model/mosaic";

/** Something both Konva and a 2D context can draw, with its size. */
export type Drawable = CanvasImageSource & ImageSize;

/** A same-origin bitmap (so the canvas stays readable) from PNG bytes. */
export function decodePng(png: Uint8Array): Promise<ImageBitmap> {
  return createImageBitmap(new Blob([png as Uint8Array<ArrayBuffer>], { type: "image/png" }));
}

export class ImageSource {
  private pixels: ImageData | null = null;

  constructor(readonly image: Drawable) {}

  get width(): number {
    return this.image.width;
  }

  get height(): number {
    return this.image.height;
  }

  private expanded(): ImageData {
    if (this.pixels === null) {
      const canvas = document.createElement("canvas");
      canvas.width = this.width;
      canvas.height = this.height;
      const context = canvas.getContext("2d", { willReadFrequently: true });
      if (context === null) throw new Error("no 2D context to read the image");
      context.drawImage(this.image, 0, 0);
      this.pixels = context.getImageData(0, 0, this.width, this.height);
    }
    return this.pixels;
  }

  /** The original pixels of `rect` (integers, inside the image). */
  regionPixels(rect: Rect): Pixels {
    const all = this.expanded();
    const data = new Uint8ClampedArray(rect.width * rect.height * 4);
    for (let row = 0; row < rect.height; row++) {
      const start = ((rect.y + row) * all.width + rect.x) * 4;
      data.set(all.data.subarray(start, start + rect.width * 4), row * rect.width * 4);
    }
    return { width: rect.width, height: rect.height, data };
  }
}
