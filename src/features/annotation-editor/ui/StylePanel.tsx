// The style of new shapes and of the selection: color, line width, text size and background.
import type { TextBackground } from "../model/document";
import { FONT_SIZES, PALETTE, STROKE_WIDTHS } from "../model/style";
import { useEditor, useEditorContext } from "../store/editor-context";

const COLOR_NAMES: Record<string, string> = {
  "#E53935": "赤",
  "#FB8C00": "橙",
  "#FDD835": "黄",
  "#43A047": "緑",
  "#1E88E5": "青",
  "#8E24AA": "紫",
  "#212121": "黒",
  "#FFFFFF": "白",
};

const BACKGROUNDS: readonly { value: TextBackground; label: string }[] = [
  { value: "none", label: "なし" },
  { value: "white", label: "白" },
  { value: "color", label: "選択色" },
];

export function StylePanel() {
  const { store } = useEditorContext();
  const style = useEditor((s) => s.style);
  const apply = store.getState().applyStyle;

  return (
    <div className="editor-style" aria-label="スタイル">
      <div className="editor-palette" role="radiogroup" aria-label="色">
        {PALETTE.map((color) => (
          <button
            key={color}
            type="button"
            role="radio"
            aria-checked={style.color.toUpperCase() === color}
            aria-label={COLOR_NAMES[color] ?? color}
            title={COLOR_NAMES[color]}
            className="editor-swatch"
            style={{ background: color }}
            onClick={() => apply({ color })}
          />
        ))}
      </div>
      <label>
        線の太さ
        <select
          value={style.strokeWidth}
          onChange={(e) => apply({ strokeWidth: Number(e.target.value) })}
        >
          {STROKE_WIDTHS.map((width) => (
            <option key={width} value={width}>
              {width}px
            </option>
          ))}
        </select>
      </label>
      <label>
        文字サイズ
        <select
          value={style.fontSize}
          onChange={(e) => apply({ fontSize: Number(e.target.value) })}
        >
          {FONT_SIZES.map((size) => (
            <option key={size} value={size}>
              {size}px
            </option>
          ))}
        </select>
      </label>
      <div className="editor-segmented" role="radiogroup" aria-label="テキストの背景">
        {BACKGROUNDS.map(({ value, label }) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={style.textBackground === value}
            onClick={() => apply({ textBackground: value })}
          >
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}
