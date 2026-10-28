// Test captures for the save folder: a solid RGBA PNG with Snipt's `snipt:meta` iTXt chunk
// right after IHDR, as capture-library writes originals (src-tauri/src/capture_library/png_meta.rs).
import { crc32, deflateSync } from "node:zlib";

export interface SeedCapture {
  /** `YYYYMMDD_HHMMSS`, matching `capturedAt`. */
  id: string;
  /** RFC 3339 with an offset, e.g. `2020-01-02T03:04:05+09:00`. */
  capturedAt: string;
  width: number;
  height: number;
  rgba: readonly [number, number, number, number];
}

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function chunk(type: string, data: Buffer): Buffer {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0);
  return Buffer.concat([head, data, crc]);
}

export function seedPng(capture: SeedCapture): Buffer {
  const { width, height, rgba } = capture;
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  // compression, filter and interlace stay 0

  const meta = JSON.stringify({
    v: 1,
    id: capture.id,
    capturedAt: capture.capturedAt,
    mode: "region",
    windowTitle: null,
  });
  // keyword \0, compression flag 0, method 0, language tag \0, translated keyword \0, text
  const itxt = Buffer.concat([
    Buffer.from("snipt:meta\0", "latin1"),
    Buffer.from([0, 0, 0, 0]),
    Buffer.from(meta, "utf8"),
  ]);

  const row = Buffer.alloc(1 + width * 4);
  for (let x = 0; x < width; x++) {
    row.set(rgba, 1 + x * 4);
  }
  const raw = Buffer.concat(Array.from({ length: height }, () => row));

  return Buffer.concat([
    SIGNATURE,
    chunk("IHDR", ihdr),
    chunk("iTXt", itxt),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** `Snipt - 2020/01/02 03:04:05`, the editor window title for an id without `_NN`. */
export function editorTitle(id: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})$/.exec(id);
  if (m === null) {
    throw new Error(`not a plain capture id: ${id}`);
  }
  const [, y, mo, d, h, mi, s] = m;
  return `Snipt - ${y}/${mo}/${d} ${h}:${mi}:${s}`;
}
