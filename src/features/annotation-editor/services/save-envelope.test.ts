import { describe, expect, it } from "vitest";
import { addShape } from "../model/operations";
import { blank, text } from "../model/testing";
import { encodeSaveEnvelope } from "./save-envelope";

describe("encodeSaveEnvelope", () => {
  it("puts the JSON length, the JSON and the PNG one after another", () => {
    const doc = addShape(blank(), text("t", "日本語のコメント\n2 行目"));
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

    const body = encodeSaveEnvelope(doc, png);

    const length = new DataView(body.buffer, body.byteOffset).getUint32(0, true);
    const json = new TextDecoder().decode(body.subarray(4, 4 + length));
    expect(JSON.parse(json)).toEqual(doc);
    // The length counts UTF-8 bytes, not characters.
    expect(length).toBeGreaterThan(json.length);
    expect(body.subarray(4 + length)).toEqual(png);
    expect(body).toHaveLength(4 + length + png.length);
  });
});
