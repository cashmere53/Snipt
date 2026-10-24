// The text box over the canvas for typing a text: at the text's place and size on screen,
// growing with its content. Enter makes a new line; Esc (outside IME composition) and leaving
// the box commit the text.
import { type CSSProperties, useEffect, useRef } from "react";
import { textAttrs } from "../canvas/shape-attrs";
import type { TextShape } from "../model/document";
import { currentDoc } from "../store/editor-store";
import { useEditor, useEditorContext } from "../store/editor-context";
import { stageScale } from "../store/view";

export function TextEditorOverlay() {
  const { store } = useEditorContext();
  const textEdit = useEditor((s) => s.textEdit);
  const style = useEditor((s) => s.style);
  const view = useEditor((s) => s.view);
  const dpr = useEditor((s) => s.devicePixelRatio);
  const editing = useEditor((s) =>
    s.textEdit?.shapeId == null
      ? undefined
      : currentDoc(s).shapes.find(
          (shape): shape is TextShape => shape.id === s.textEdit?.shapeId && shape.kind === "text",
        ),
  );
  const box = useRef<HTMLTextAreaElement>(null);
  const composing = useRef(false);

  const open = textEdit !== null;
  useEffect(() => {
    if (open) box.current?.focus();
  }, [open]);

  if (textEdit === null) return null;

  // The look of the text as it will be drawn, from the same mapping as the canvas.
  const attrs = textAttrs({
    id: "",
    kind: "text",
    position: textEdit.position,
    text: textEdit.draft,
    color: editing?.color ?? style.color,
    fontSize: editing?.fontSize ?? style.fontSize,
    background: editing?.background ?? style.textBackground,
  });
  const scale = stageScale(view.zoom, dpr);
  const fontSize = (attrs.text.fontSize ?? 0) * scale;
  const padding = (attrs.text.padding ?? 0) * scale;
  const css: CSSProperties = {
    left: view.pan.x + textEdit.position.x * scale,
    top: view.pan.y + textEdit.position.y * scale,
    fontSize,
    fontFamily: attrs.text.fontFamily,
    lineHeight: attrs.text.lineHeight,
    padding,
    color: attrs.text.fill as string,
    background: (attrs.tag.fill as string | undefined) ?? "transparent",
    borderRadius: (attrs.tag.cornerRadius as number) * scale,
    minWidth: fontSize,
  };

  const commit = () => store.getState().commitTextEdit();

  return (
    <textarea
      ref={box}
      className="editor-text-input"
      aria-label="テキスト"
      style={css}
      value={textEdit.draft}
      rows={Math.max(1, textEdit.draft.split("\n").length)}
      spellCheck={false}
      onChange={(e) => store.getState().updateTextDraft(e.target.value)}
      onCompositionStart={() => (composing.current = true)}
      onCompositionEnd={() => (composing.current = false)}
      onKeyDown={(e) => {
        // Keys stay in the box: the editor's shortcuts do not see them.
        e.stopPropagation();
        if (e.key === "Escape" && !composing.current && !e.nativeEvent.isComposing) {
          e.preventDefault();
          commit();
        }
      }}
      onBlur={commit}
    />
  );
}
