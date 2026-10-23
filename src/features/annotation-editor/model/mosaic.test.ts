import { describe, expect, it } from "vitest";
import { mosaicBlockSize, type Pixels, pixelate } from "./mosaic";

function pixels(width: number, height: number, at: (x: number, y: number) => number[]): Pixels {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      data.set(at(x, y), (y * width + x) * 4);
    }
  }
  return { width, height, data };
}

function pixelAt(p: Pixels, x: number, y: number): number[] {
  const i = (y * p.width + x) * 4;
  return Array.from(p.data.subarray(i, i + 4));
}

describe("mosaicBlockSize", () => {
  it("is at least 16 and grows with the longer side", () => {
    expect(mosaicBlockSize({ width: 800, height: 600 })).toBe(16);
    expect(mosaicBlockSize({ width: 1920, height: 1080 })).toBe(16);
    expect(mosaicBlockSize({ width: 3840, height: 2160 })).toBe(32);
    expect(mosaicBlockSize({ width: 2160, height: 7680 })).toBe(64);
  });
});

describe("pixelate", () => {
  // A checkerboard of black and white with translucent pixels: every block average is a
  // gray that none of the source pixels has.
  const src = pixels(20, 12, (x, y) => ((x + y) % 2 === 0 ? [0, 0, 0, 255] : [255, 255, 255, 128]));
  const out = pixelate(src, 8);

  it("fills each block with one opaque color", () => {
    const blocks = [
      [0, 0, 8, 8],
      [8, 0, 16, 8],
      [16, 0, 20, 8],
      [0, 8, 8, 12],
      [16, 8, 20, 12],
    ];
    for (const [left, top, right, bottom] of blocks) {
      const first = pixelAt(out, left!, top!);
      expect(first[3]).toBe(255);
      for (let y = top!; y < bottom!; y++) {
        for (let x = left!; x < right!; x++) {
          expect(pixelAt(out, x, y)).toEqual(first);
        }
      }
    }
  });

  it("keeps no original pixel value", () => {
    const originals = new Set<string>();
    for (let y = 0; y < src.height; y++) {
      for (let x = 0; x < src.width; x++) {
        originals.add(pixelAt(src, x, y).join());
      }
    }
    for (let y = 0; y < out.height; y++) {
      for (let x = 0; x < out.width; x++) {
        expect(originals.has(pixelAt(out, x, y).join())).toBe(false);
      }
    }
    expect(pixelAt(out, 0, 0)).toEqual([128, 128, 128, 255]);
  });

  it("does not change its input", () => {
    expect(pixelAt(src, 1, 0)).toEqual([255, 255, 255, 128]);
    expect(out.data).not.toBe(src.data);
  });
});
