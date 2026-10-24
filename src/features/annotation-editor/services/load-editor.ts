// Opens an editor window's capture: the session, the original image and the stored
// annotations. Unreadable annotations open the original alone, with a warning; a session or
// an image that cannot be read leaves nothing to edit.
import type { EditorSessionDto } from "@/shared/ipc/annotation-editor";
import { type AnnotationDocument, emptyDocument } from "../model/document";
import { parseDocument } from "../model/parse-document";
import type { EditorIpc } from "./editor-ipc";
import { decodePng, type Drawable, ImageSource } from "./image-source";

export type LoadedEditor =
  | {
      ok: true;
      session: EditorSessionDto;
      source: ImageSource;
      doc: AnnotationDocument;
      /** Shown as banners; the editor works anyway. */
      warnings: string[];
    }
  | { ok: false; message: string };

export const UNREADABLE_ANNOTATIONS =
  "保存済みの注釈を読み込めなかったため、元画像のみで開きました。";
export const NEWER_ANNOTATIONS =
  "保存済みの注釈は新しいバージョンの Snipt で作られたため、元画像のみで開きました。";
export const UNREADABLE_IMAGE = "元画像を読み込めませんでした。";

export async function loadEditor(
  ipc: Pick<EditorIpc, "openSession" | "readOriginal">,
  decode: (png: Uint8Array) => Promise<Drawable> = decodePng,
): Promise<LoadedEditor> {
  const session = await ipc.openSession();
  if (!session.ok) return { ok: false, message: session.error.message };
  const original = await ipc.readOriginal();
  if (!original.ok) return { ok: false, message: original.error.message };
  let image: Drawable;
  try {
    image = await decode(original.value);
  } catch {
    return { ok: false, message: UNREADABLE_IMAGE };
  }

  const size = { width: image.width, height: image.height };
  const warnings = session.value.warning === null ? [] : [session.value.warning];
  let doc = emptyDocument(size);
  const { annotations } = session.value;
  if (annotations !== null && annotations !== undefined) {
    const parsed = parseDocument(annotations, size);
    if (parsed.ok) doc = parsed.doc;
    else
      warnings.push(
        parsed.reason === "unsupportedVersion" ? NEWER_ANNOTATIONS : UNREADABLE_ANNOTATIONS,
      );
  }
  return { ok: true, session: session.value, source: new ImageSource(image), doc, warnings };
}
