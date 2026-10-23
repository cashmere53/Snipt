// Display text for history tiles.

const RFC3339 = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})/;

/**
 * `yyyy/MM/dd HH:mm:ss` in the clock time of the capture (the offset stored with it), so a
 * capture shows the time it was taken wherever the history is opened.
 */
export function formatCapturedAt(capturedAt: string): string {
  const match = RFC3339.exec(capturedAt);
  if (match === null) {
    return capturedAt;
  }
  const [, year, month, day, hour, minute, second] = match;
  return `${year}/${month}/${day} ${hour}:${minute}:${second}`;
}

/** `幅 × 高さ` in pixels. */
export function formatSize(width: number, height: number): string {
  return `${width} × ${height}`;
}
