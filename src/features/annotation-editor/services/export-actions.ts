// Copy and "save as": the composite of the current document, at the original resolution of
// the crop. Each returns the reason it failed, or `null`; the document is never changed.
import type { ExportFormat } from "@/shared/ipc/annotation-editor";
import type { AnnotationDocument } from "../model/document";
import { type RenderResult, renderErrorMessage } from "./composite-renderer";
import type { EditorIpc } from "./editor-ipc";

export type Render = (doc: AnnotationDocument, format: ExportFormat) => Promise<RenderResult>;

export async function copyComposite(
  doc: AnnotationDocument,
  render: Render,
  ipc: Pick<EditorIpc, "copy">,
): Promise<string | null> {
  const composite = await render(doc, "png");
  if (!composite.ok) return renderErrorMessage(composite.error);
  const copied = await ipc.copy(composite.bytes);
  return copied.ok ? null : copied.error.message;
}

/** Asks where to save (cancelling does nothing), then writes in the chosen format. */
export async function exportComposite(
  doc: AnnotationDocument,
  render: Render,
  ipc: Pick<EditorIpc, "pickExportPath" | "writeExport">,
  suggestedName: string,
): Promise<string | null> {
  const target = await ipc.pickExportPath(suggestedName);
  if (!target.ok) return target.error.message;
  if (target.value === null) return null;
  const composite = await render(doc, target.value.format);
  if (!composite.ok) return renderErrorMessage(composite.error);
  const written = await ipc.writeExport(target.value.token, composite.bytes);
  return written.ok ? null : written.error.message;
}

/**
 * The file name offered by the save dialog. Unlike `Snipt_<id>.png` and
 * `Snipt_<id>_annotated.png`, it never replaces a file of the capture in the save folder.
 */
export const suggestedExportName = (captureId: string) => `Snipt_${captureId}_export.png`;
