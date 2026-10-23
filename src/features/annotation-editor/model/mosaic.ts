// The mosaic: replaces each block with its average color, opaque, so no original pixel value
// survives (a blur could be reversed).
import type { ImageSize } from "./document";

export const MIN_MOSAIC_BLOCK = 16;

/** Grows with the image so that text stays unreadable when zoomed out. */
export function mosaicBlockSize(image: ImageSize): number {
  return Math.max(MIN_MOSAIC_BLOCK, Math.round(Math.max(image.width, image.height) / 120));
}

/** RGBA pixels, row by row (the fields of `ImageData`). */
export interface Pixels {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

/**
 * A copy of `src` where each `blockSize` square (aligned to the top left; partial at the right
 * and bottom edges) is filled with the average of its pixels, fully opaque.
 */
export function pixelate(src: Pixels, blockSize: number): Pixels {
  const { width, height } = src;
  const data = new Uint8ClampedArray(width * height * 4);
  for (let top = 0; top < height; top += blockSize) {
    const bottom = Math.min(top + blockSize, height);
    for (let left = 0; left < width; left += blockSize) {
      const right = Math.min(left + blockSize, width);
      let r = 0;
      let g = 0;
      let b = 0;
      for (let y = top; y < bottom; y++) {
        for (let x = left; x < right; x++) {
          const i = (y * width + x) * 4;
          r += src.data[i]!;
          g += src.data[i + 1]!;
          b += src.data[i + 2]!;
        }
      }
      const count = (bottom - top) * (right - left);
      const color = [Math.round(r / count), Math.round(g / count), Math.round(b / count), 255];
      for (let y = top; y < bottom; y++) {
        for (let x = left; x < right; x++) {
          data.set(color, (y * width + x) * 4);
        }
      }
    }
  }
  return { width, height, data };
}
