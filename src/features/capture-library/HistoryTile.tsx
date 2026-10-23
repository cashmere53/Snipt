import type { MouseEvent } from "react";
import { type CaptureId, type CaptureItem, thumbnailSrc } from "@/shared/ipc/capture-library";
import { formatCapturedAt, formatSize } from "./format";
import type { Modifiers } from "./history-selection";

export interface TileActions {
  onSelect: (id: CaptureId, modifiers: Modifiers) => void;
  onOpen: (id: CaptureId) => void;
  /** The thumbnail could not be loaded: the capture may be gone (requirement 3.7). */
  onMissing: (id: CaptureId) => void;
  onContextMenu: (id: CaptureId, x: number, y: number) => void;
}

export function HistoryTile({
  item,
  selected,
  actions,
}: {
  item: CaptureItem;
  selected: boolean;
  actions: TileActions;
}) {
  const src = thumbnailSrc(item);
  const date = formatCapturedAt(item.capturedAt);
  const size = formatSize(item.width, item.height);

  const select = (event: MouseEvent) =>
    actions.onSelect(item.id, { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey });

  return (
    <div
      role="option"
      aria-selected={selected}
      aria-label={`${date} ${size}`}
      className="history-tile"
      onClick={select}
      onDoubleClick={() => actions.onOpen(item.id)}
      onContextMenu={(event) => {
        event.preventDefault();
        actions.onContextMenu(item.id, event.clientX, event.clientY);
      }}
    >
      <div className="history-thumb">
        {src === null ? (
          <div className="history-thumb-placeholder" aria-hidden="true" />
        ) : (
          <img
            src={src}
            alt=""
            draggable={false}
            loading="lazy"
            onError={() => actions.onMissing(item.id)}
          />
        )}
      </div>
      <div className="history-caption">
        <time dateTime={item.capturedAt}>{date}</time>
        <span>{size}</span>
      </div>
    </div>
  );
}
