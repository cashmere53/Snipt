// Style settings for new shapes and the selection, and their stored form's validation.
import { type HexColor, isHexColor, TEXT_BACKGROUNDS, type TextBackground } from "./document";

export interface StyleSettings {
  color: HexColor;
  strokeWidth: number;
  fontSize: number;
  textBackground: TextBackground;
}

export const PALETTE: readonly HexColor[] = [
  "#E53935", // red
  "#FB8C00", // orange
  "#FDD835", // yellow
  "#43A047", // green
  "#1E88E5", // blue
  "#8E24AA", // purple
  "#212121", // black
  "#FFFFFF", // white
];
export const STROKE_WIDTHS: readonly number[] = [2, 4, 6, 8, 12];
export const FONT_SIZES: readonly number[] = [14, 18, 24, 32, 48, 64];

export const DEFAULT_STYLE: StyleSettings = {
  color: "#E53935",
  strokeWidth: 4,
  fontSize: 24,
  textBackground: "none",
};

function field(raw: unknown, key: keyof StyleSettings): unknown {
  return typeof raw === "object" && raw !== null
    ? (raw as Record<string, unknown>)[key]
    : undefined;
}

function oneOf<T>(value: unknown, choices: readonly T[], fallback: T): T {
  return choices.includes(value as T) ? (value as T) : fallback;
}

/** Keeps each valid field of a stored style and replaces the others with the defaults. */
export function validateStyle(raw: unknown): StyleSettings {
  const color = field(raw, "color");
  return {
    color: isHexColor(color) ? color : DEFAULT_STYLE.color,
    strokeWidth: oneOf(field(raw, "strokeWidth"), STROKE_WIDTHS, DEFAULT_STYLE.strokeWidth),
    fontSize: oneOf(field(raw, "fontSize"), FONT_SIZES, DEFAULT_STYLE.fontSize),
    textBackground: oneOf(
      field(raw, "textBackground"),
      TEXT_BACKGROUNDS,
      DEFAULT_STYLE.textBackground,
    ),
  };
}
