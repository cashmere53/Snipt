// Reads a stored annotation document: migrates older versions one step at a time, then
// checks every field. A document that fails is not used (the editor opens without it).
import {
  type AnnotationDocument,
  CURRENT_SCHEMA_VERSION,
  type ImageSize,
  isHexColor,
  type Point,
  type Rect,
  type Shape,
  TEXT_BACKGROUNDS,
  type TextBackground,
} from "./document";
import { MIN_MOSAIC_BLOCK } from "./mosaic";

export type ParseResult =
  | { ok: true; doc: AnnotationDocument; migrated: boolean }
  | { ok: false; reason: "unsupportedVersion" | "invalid" };

/** `migrations[v]` turns a version `v` document into version `v + 1`. */
export type Migrations = Readonly<Record<number, (doc: Record<string, unknown>) => unknown>>;

/** None yet: version 1 is the first. */
export const MIGRATIONS: Migrations = {};

/** Thrown by the field readers below; caught by `parseDocument`. */
export class Invalid extends Error {}

type Fields = Record<string, unknown>;

function object(value: unknown): Fields {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Invalid();
  }
  return value as Fields;
}

function number(value: unknown, check: (n: number) => boolean = () => true): number {
  if (typeof value !== "number" || !Number.isFinite(value) || !check(value)) {
    throw new Invalid();
  }
  return value;
}

const positive = (n: number) => n > 0;
const wholeAtLeast = (min: number) => (n: number) => Number.isInteger(n) && n >= min;

function point(value: unknown): Point {
  const p = object(value);
  return { x: number(p.x), y: number(p.y) };
}

function rect(value: unknown): Rect {
  const r = object(value);
  return {
    x: number(r.x),
    y: number(r.y),
    width: number(r.width, positive),
    height: number(r.height, positive),
  };
}

/** A whole-pixel rectangle inside the image (crop, mosaic). */
function imageRect(value: unknown, image: ImageSize): Rect {
  const r = rect(value);
  const whole = [r.x, r.y, r.width, r.height].every(Number.isInteger);
  if (
    !whole ||
    r.x < 0 ||
    r.y < 0 ||
    r.x + r.width > image.width ||
    r.y + r.height > image.height
  ) {
    throw new Invalid();
  }
  return r;
}

function color(value: unknown) {
  if (!isHexColor(value)) {
    throw new Invalid();
  }
  return value;
}

function shape(value: unknown, image: ImageSize): Shape {
  const s = object(value);
  if (typeof s.id !== "string" || s.id === "") {
    throw new Invalid();
  }
  const id = s.id;
  switch (s.kind) {
    case "arrow":
      return {
        id,
        kind: "arrow",
        start: point(s.start),
        end: point(s.end),
        color: color(s.color),
        strokeWidth: number(s.strokeWidth, positive),
      };
    case "rect":
      return {
        id,
        kind: "rect",
        rect: rect(s.rect),
        color: color(s.color),
        strokeWidth: number(s.strokeWidth, positive),
      };
    case "text":
      if (typeof s.text !== "string" || s.text.trim() === "") {
        throw new Invalid();
      }
      if (!TEXT_BACKGROUNDS.includes(s.background as TextBackground)) {
        throw new Invalid();
      }
      return {
        id,
        kind: "text",
        position: point(s.position),
        text: s.text,
        color: color(s.color),
        fontSize: number(s.fontSize, positive),
        background: s.background as TextBackground,
      };
    case "mosaic":
      return {
        id,
        kind: "mosaic",
        rect: imageRect(s.rect, image),
        blockSize: number(s.blockSize, wholeAtLeast(MIN_MOSAIC_BLOCK)),
      };
    case "marker":
      return {
        id,
        kind: "marker",
        center: point(s.center),
        number: number(s.number, wholeAtLeast(1)),
        color: color(s.color),
        size: number(s.size, positive),
      };
    default:
      throw new Invalid();
  }
}

/**
 * Applies `migrations[from]`, `migrations[from + 1]`, ... up to version `to`. Exported for
 * tests; a missing step makes the document invalid.
 */
export function migrateDocument(
  doc: Record<string, unknown>,
  from: number,
  to: number,
  migrations: Migrations,
): Record<string, unknown> {
  let current = doc;
  for (let version = from; version < to; version++) {
    const step = migrations[version];
    if (step === undefined) {
      throw new Invalid();
    }
    current = object(step(current));
  }
  return current;
}

/** Checks a current-version document against the image it belongs to. */
function validate(raw: Fields, image: ImageSize): AnnotationDocument {
  const size = object(raw.image);
  if (size.width !== image.width || size.height !== image.height) {
    throw new Invalid();
  }
  if (!Array.isArray(raw.shapes)) {
    throw new Invalid();
  }
  const shapes = raw.shapes.map((s) => shape(s, image));
  if (new Set(shapes.map((s) => s.id)).size !== shapes.length) {
    throw new Invalid();
  }
  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    image: { width: image.width, height: image.height },
    crop: raw.crop === null ? null : imageRect(raw.crop, image),
    shapes,
  };
}

/**
 * Turns a stored document into the current version and checks it against `image`. Unknown
 * fields are dropped.
 */
export function parseDocument(
  raw: unknown,
  image: ImageSize,
  migrations: Migrations = MIGRATIONS,
): ParseResult {
  try {
    const doc = object(raw);
    const version = number(doc.schemaVersion, wholeAtLeast(1));
    if (version > CURRENT_SCHEMA_VERSION) {
      return { ok: false, reason: "unsupportedVersion" };
    }
    const current = migrateDocument(doc, version, CURRENT_SCHEMA_VERSION, migrations);
    return { ok: true, doc: validate(current, image), migrated: version < CURRENT_SCHEMA_VERSION };
  } catch (error) {
    if (error instanceof Invalid) {
      return { ok: false, reason: "invalid" };
    }
    throw error;
  }
}
