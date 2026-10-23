// The canvas package loads its native libraries and starts fontconfig on the first 2D context,
// which takes several seconds on Windows CI. Test files that draw call this in `beforeAll`
// with `CANVAS_WARM_UP_TIMEOUT`, so that one-time cost does not count against a test.
export const CANVAS_WARM_UP_TIMEOUT = 60_000;

export function warmUpCanvas(): void {
  document.createElement("canvas").getContext("2d");
}
