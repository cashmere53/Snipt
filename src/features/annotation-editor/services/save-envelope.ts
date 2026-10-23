// The body of `editor_save`: the annotation document and the composite PNG in one request,
// as `[u32 LE: JSON length][UTF-8 JSON][PNG]`.
import type { AnnotationDocument } from "../model/document";

export function encodeSaveEnvelope(doc: AnnotationDocument, png: Uint8Array): Uint8Array {
  const json = new TextEncoder().encode(JSON.stringify(doc));
  const body = new Uint8Array(4 + json.length + png.length);
  new DataView(body.buffer).setUint32(0, json.length, true);
  body.set(json, 4);
  body.set(png, 4 + json.length);
  return body;
}
