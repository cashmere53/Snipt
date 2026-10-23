// Messages that do not change the document: unreadable annotations, failed copies and exports.
export interface Notice {
  id: number;
  level: "warning" | "error";
  message: string;
}

export function NoticeBanner({
  notices,
  onDismiss,
}: {
  notices: readonly Notice[];
  onDismiss(id: number): void;
}) {
  if (notices.length === 0) return null;
  return (
    <div className="editor-notices">
      {notices.map((notice) => (
        <div
          key={notice.id}
          className={`editor-notice editor-notice-${notice.level}`}
          role={notice.level === "error" ? "alert" : "status"}
        >
          <span>{notice.message}</span>
          <button type="button" aria-label="閉じる" onClick={() => onDismiss(notice.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
