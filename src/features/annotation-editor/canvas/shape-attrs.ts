// Shapes to Konva node settings: the one mapping that both the editor view and the composite
// export use, so that what is shown is what is exported. Coordinates stay in image pixels.
import type Konva from "konva";
import type {
  ArrowShape,
  HexColor,
  MarkerShape,
  MosaicShape,
  RectShape,
  TextShape,
} from "../model/document";

export const FONT_FAMILY = '"Segoe UI", "Yu Gothic UI", Meiryo, sans-serif';
/** Space between a text's background and its letters, relative to the font size. */
const TEXT_PADDING = 0.25;
const MARKER_FONT_SHARE = 0.55;

const WHITE: HexColor = "#FFFFFF";
const BLACK: HexColor = "#000000";

/** WCAG relative luminance of `#RRGGBB`. */
export function luminance(color: HexColor): number {
  const channel = (offset: number) => {
    const value = Number.parseInt(color.slice(offset, offset + 2), 16) / 255;
    return value <= 0.039_28 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

/**
 * Black on light colors, white otherwise. The threshold sits above the point where black
 * would contrast slightly more (0.18), so saturated colors such as the palette's red, orange,
 * green and blue keep white letters like signs do.
 */
export function contrastingTextColor(background: HexColor): HexColor {
  return luminance(background) > 0.4 ? BLACK : WHITE;
}

export function arrowAttrs(shape: ArrowShape): Konva.ArrowConfig {
  const head = Math.max(10, shape.strokeWidth * 3);
  return {
    points: [shape.start.x, shape.start.y, shape.end.x, shape.end.y],
    stroke: shape.color,
    fill: shape.color,
    strokeWidth: shape.strokeWidth,
    pointerLength: head,
    pointerWidth: head,
    pointerAtBeginning: false,
    pointerAtEnding: true,
    lineCap: "round",
    lineJoin: "round",
  };
}

export function rectAttrs(shape: RectShape): Konva.RectConfig {
  return {
    ...shape.rect,
    stroke: shape.color,
    strokeWidth: shape.strokeWidth,
    fillEnabled: false,
    lineJoin: "miter",
  };
}

export interface TextAttrs {
  /** The `Konva.Label` group, placed at the text's top left. */
  label: Konva.LabelConfig;
  /** The background; `fill` is undefined without one. */
  tag: Konva.TagConfig;
  text: Konva.TextConfig;
}

export function textAttrs(shape: TextShape): TextAttrs {
  const fills: Record<TextShape["background"], { tag?: HexColor; text: HexColor }> = {
    none: { text: shape.color },
    white: { tag: WHITE, text: shape.color },
    color: { tag: shape.color, text: contrastingTextColor(shape.color) },
  };
  const fill = fills[shape.background];
  return {
    label: { x: shape.position.x, y: shape.position.y },
    tag: { fill: fill.tag, cornerRadius: Math.round(shape.fontSize * 0.15) },
    text: {
      text: shape.text,
      fontSize: shape.fontSize,
      fontFamily: FONT_FAMILY,
      lineHeight: 1.25,
      padding: Math.round(shape.fontSize * TEXT_PADDING),
      fill: fill.text,
      wrap: "none",
    },
  };
}

export interface MarkerAttrs {
  group: Konva.ContainerConfig;
  circle: Konva.CircleConfig;
  label: Konva.TextConfig;
}

/** A filled circle around `center` with its number in the middle. */
export function markerAttrs(shape: MarkerShape): MarkerAttrs {
  const radius = shape.size / 2;
  return {
    group: { x: shape.center.x, y: shape.center.y },
    circle: { x: 0, y: 0, radius, fill: shape.color, stroke: WHITE, strokeWidth: 2 },
    label: {
      x: -radius,
      y: -radius,
      width: shape.size,
      height: shape.size,
      text: String(shape.number),
      fontSize: Math.round(shape.size * MARKER_FONT_SHARE),
      fontFamily: FONT_FAMILY,
      fontStyle: "bold",
      align: "center",
      verticalAlign: "middle",
      fill: contrastingTextColor(shape.color),
    },
  };
}

/**
 * Where the pixelated block image goes, in integer pixels. The image itself (`pixelate` of the
 * original pixels) is the caller's; its layer draws it without smoothing.
 */
export function mosaicAttrs(shape: MosaicShape): Omit<Konva.ImageConfig, "image"> {
  const { x, y, width, height } = shape.rect;
  return { x, y, width, height };
}
