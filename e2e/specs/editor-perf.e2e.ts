// annotation-editor 7.3: performance on a real Windows desktop with an 11520×2160 capture. Times
// the open (double-click to the window shown, then to the first image on the canvas), the composite
// PNG (Ctrl+C to the copy request, which is the render and the encode) and the frame rate while a
// shape is dragged over the image. Requirements 1.1, 11.1 (opening), 10.3 (the composite PNG).
// Each run prints `E2E perf:` lines; the targets are asserted, so an overage fails the run and the
// line shows by how much.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  at,
  drag,
  editorLabels,
  imageBox,
  type Point,
  pressButton,
  pressCtrl,
  savedDocument,
  switchTo,
  waitUntil,
} from "../support/app";
import { Desktop } from "../support/desktop";
import { abortSessionStart, type Session, startSession } from "../support/driver";
import { editorTitle, type SeedCapture } from "../support/png";

const WIDE: SeedCapture = {
  id: "20200102_030410",
  capturedAt: "2020-01-02T03:04:10+09:00",
  width: 11520,
  height: 2160,
  rgba: [200, 200, 200, 255],
  pattern: "gradient",
};
const HISTORY_TITLE = "Snipt 履歴";
/** The targets of design.md (Testing Strategy → Performance). */
const TARGET = { openMs: 700, compositeMs: 2500, fps: 60, fpsFloor: 55 };
/** The drag: this many pointer moves, this far apart in time (about 2 s in all). */
const DRAG_STEPS = 120;
const DRAG_STEP_MS = 16;

/** What the instrumented editor page records (see `instrumentCopy`). */
interface CopyTiming {
  keyAt: number | null;
  /** The `editor_copy` request starts (the render and the encode are before it). */
  copyAt: number | null;
  /** The `editor_copy` response arrives (the copy is done in the editor). */
  copyDoneAt: number | null;
  encodes: { type: string; ms: number; bytes: number | null }[];
  calls: string[];
  errors: string[];
}

/** The per-page measurement state: the test's own, not the product's. */
interface PerfWindow {
  perfCopy: CopyTiming;
  perfFrames: number[];
  perfCounting: boolean;
  perfFramesStarted: boolean;
}

let session: Session;
let desktop: Desktop;
let browser: WebdriverIO.Browser;

/** The history tile's accessible name: `yyyy/MM/dd HH:mm:ss 幅 × 高さ`. */
function tileLabel(capture: SeedCapture): string {
  const [date, time] = capture.capturedAt.slice(0, 19).split("T") as [string, string];
  return `${date.replaceAll("-", "/")} ${time} ${capture.width} × ${capture.height}`;
}

async function waitForForeground(title: string): Promise<void> {
  let last = "";
  await waitUntil(`"${title}" in the foreground`, async () => {
    last = await desktop.foregroundTitle();
    return last === title;
  }).catch((e: unknown) => {
    throw new Error(`${String(e)} (foreground: "${last}")`);
  });
}

/** Brings the editor `label` (titled `title`) to the front and makes it the WebDriver target. */
async function focusEditor(label: string, title: string): Promise<void> {
  await switchTo(browser, label);
  await desktop.activate(title);
  await waitForForeground(title);
}

/**
 * Polls as fast as the probe allows (`waitUntil` sleeps 200 ms, too coarse for a 700 ms budget)
 * and returns the time the probe first succeeded, on the `performance.now()` clock.
 */
async function pollFast(
  what: string,
  probe: () => Promise<boolean>,
  timeoutMs = 60_000,
): Promise<number> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (await probe()) {
      return performance.now();
    }
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for ${what}`);
    }
    await new Promise((r) => setTimeout(r, 10));
  }
}

/**
 * Ctrl+C timing in the page: the key press, the copy request, its response, and each canvas PNG
 * encode. Tauri 2 sends each command as a `fetch` POST to the IPC URL that ends in the command
 * name. `__TAURI_INTERNALS__.invoke` itself is read-only, so the request is seen on `fetch`.
 */
async function instrumentCopy(): Promise<void> {
  await browser.execute(() => {
    const w = window as unknown as PerfWindow;
    w.perfCopy = {
      keyAt: null,
      copyAt: null,
      copyDoneAt: null,
      encodes: [],
      calls: [],
      errors: [],
    };
    window.addEventListener(
      "keydown",
      (e) => {
        if (e.code === "KeyC" && (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
          w.perfCopy.keyAt = performance.now();
        }
      },
      true,
    );
    const fetchOriginal = window.fetch;
    window.fetch = function (input, init) {
      const href =
        typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      let command: string | null = null;
      try {
        const url = new URL(href, location.href);
        if (url.hostname === "ipc.localhost" || url.protocol === "ipc:") {
          command = url.pathname.replace(/^\//, "");
        }
      } catch {
        // not a URL: not an IPC request
      }
      if (command === null) {
        return fetchOriginal.call(window, input, init);
      }
      const started = performance.now();
      w.perfCopy.calls.push(`${command}@${Math.round(started)}`);
      const isCopy = command === "editor_copy";
      if (isCopy) {
        w.perfCopy.copyAt = started;
      }
      const response = fetchOriginal.call(window, input, init);
      if (isCopy) {
        response.then(
          () => {
            w.perfCopy.copyDoneAt = performance.now();
          },
          (e: unknown) => w.perfCopy.errors.push(`editor_copy: ${String(e)}`),
        );
      }
      return response;
    };
    window.addEventListener("error", (e) => w.perfCopy.errors.push(`error: ${e.message}`));
    window.addEventListener("unhandledrejection", (e) =>
      w.perfCopy.errors.push(`rejection: ${String(e.reason)}`),
    );
    // The encode of the composite is `canvas.toBlob`; its duration is recorded on the way out.
    const toBlob = HTMLCanvasElement.prototype.toBlob;
    HTMLCanvasElement.prototype.toBlob = function (callback, type, quality) {
      const started = performance.now();
      toBlob.call(
        this,
        (blob) => {
          w.perfCopy.encodes.push({
            type: type ?? "image/png",
            ms: performance.now() - started,
            bytes: blob?.size ?? null,
          });
          callback(blob);
        },
        type,
        quality,
      );
    };
  });
}

/** Counts animation frames in the page, from the first call on (the page's own clock). */
async function startFrameCount(): Promise<void> {
  await browser.execute(() => {
    const w = window as unknown as PerfWindow;
    w.perfFrames = [];
    w.perfCounting = false;
    if (!w.perfFramesStarted) {
      w.perfFramesStarted = true;
      const tick = (now: number) => {
        if (w.perfCounting) w.perfFrames.push(now);
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    }
  });
}

/** A drag in many small steps, so the editor gets pointer moves over the whole drag. */
async function slowDrag(from: Point, to: Point): Promise<void> {
  let action = browser
    .action("pointer", { parameters: { pointerType: "mouse" } })
    .move({ x: from.x, y: from.y, origin: "viewport" })
    .down({ button: 0 });
  for (let i = 1; i <= DRAG_STEPS; i++) {
    action = action.move({
      x: Math.round(from.x + ((to.x - from.x) * i) / DRAG_STEPS),
      y: Math.round(from.y + ((to.y - from.y) * i) / DRAG_STEPS),
      duration: DRAG_STEP_MS,
      origin: "viewport",
    });
  }
  await action.up({ button: 0 }).perform();
}

function report(line: string): void {
  // stderr, not console: vitest prints console output only for a failing file.
  process.stderr.write(`E2E perf: ${line}\n`);
}

beforeAll(async () => {
  session = await startSession([WIDE]);
  browser = session.browser;
  desktop = new Desktop();
  await waitUntil(
    "the 11520×2160 capture in the history",
    async () => await browser.$(`[role="option"][aria-label="${tileLabel(WIDE)}"]`).isExisting(),
    120_000,
  );
});

afterAll(async () => {
  await abortSessionStart();
  desktop?.dispose();
  await session?.close();
});

describe("annotation-editor performance E2E (11520×2160)", () => {
  const label = `editor-${WIDE.id}`;
  const title = editorTitle(WIDE.id);

  it("履歴から開く要求からウィンドウ表示まで 700ms 以内、画像が出るまでを計測する (1.1, 11.1)", async () => {
    await desktop.activate(HISTORY_TITLE);
    await waitForForeground(HISTORY_TITLE);
    await switchTo(browser, "history");
    const tile = await browser.$(`[role="option"][aria-label="${tileLabel(WIDE)}"]`);
    await tile.waitForExist({ timeout: 60_000 });
    const before = await editorLabels(browser);
    expect(before).not.toContain(label);

    // The clock starts just before the double-click is sent, so the WebDriver round trip is
    // included: the figure is an upper bound on the open request.
    const started = performance.now();
    await tile.doubleClick();
    // The window is shown when it exists on the desktop with its title (`desktop.windowExists`
    // checks a visible top-level window), which is what the user sees.
    const shownAt = await pollFast("the editor window to be shown", () =>
      desktop.windowExists(title),
    );
    const shownMs = shownAt - started;
    report(`open → window shown = ${shownMs.toFixed(0)} ms (target ≤ ${TARGET.openMs} ms)`);

    await switchTo(browser, label);
    const imageAt = await pollFast(
      "the image on the canvas",
      async () => (await imageBox(browser)) !== null,
    );
    report(
      `open → first image on the canvas = ${(imageAt - started).toFixed(0)} ms (not a target)`,
    );

    await focusEditor(label, title);
    expect(
      shownMs,
      `window shown ${shownMs.toFixed(0)} ms, over by ${(shownMs - TARGET.openMs).toFixed(0)} ms`,
    ).toBeLessThanOrEqual(TARGET.openMs);
  });

  it("注釈を付けた 11520×2160 の画像で、Ctrl+C の合成 PNG 生成を 2.5 秒以内で計測する (10.3)", async () => {
    await switchTo(browser, label);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));
    // Annotations on the original, so the composite draws the shapes and the mosaic too.
    await pressButton(browser, "矢印 (A)");
    await drag(browser, at(box, 0.1, 0.2), at(box, 0.3, 0.3));
    await pressButton(browser, "四角 (R)");
    await drag(browser, at(box, 0.35, 0.2), at(box, 0.5, 0.4));
    await pressButton(browser, "ぼかし (B)");
    await drag(browser, at(box, 0.55, 0.5), at(box, 0.7, 0.7));
    await waitUntil(
      "the annotations saved",
      async () => savedDocument(session.workspace.saveDir, WIDE.id)?.shapes.length === 3,
    );

    await instrumentCopy();
    await desktop.clearClipboard();
    await focusEditor(label, title);
    // WebDriver's clock, for the clipboard figure below (the page's clock is not the same).
    const pressedAt = performance.now();
    await pressCtrl(browser, "c");
    const timing = await waitUntil(
      "the copy request and its response",
      () =>
        browser.execute(() => {
          const copy = (window as unknown as PerfWindow).perfCopy;
          return copy.copyAt !== null && copy.copyDoneAt !== null && copy.keyAt !== null
            ? copy
            : null;
        }),
      30_000,
    ).catch(async (e: unknown) => {
      // Diagnostics for a copy that never completed: the timing so far and the page's text.
      const state = await browser.execute(() => ({
        timing: (window as unknown as PerfWindow).perfCopy,
        text: document.body.innerText.slice(0, 400),
      }));
      throw new Error(`${String(e)}; ${JSON.stringify(state)}`);
    });
    const copied = await waitUntil(
      "an image on the clipboard",
      () => desktop.clipboardImage(),
      60_000,
    );
    const clipboardMs = performance.now() - pressedAt;

    const compositeMs = timing.copyAt! - timing.keyAt!;
    const copyDoneMs = timing.copyDoneAt! - timing.keyAt!;
    const pngEncode = timing.encodes.filter((e) => e.type === "image/png").at(-1);
    report(
      `composite render (Ctrl+C → editor_copy sent) = ${compositeMs.toFixed(0)} ms (target ≤ ${TARGET.compositeMs})`,
    );
    report(`composite Ctrl+C → copy done = ${copyDoneMs.toFixed(0)} ms (not a target)`);
    report(
      `canvas PNG encode = ${pngEncode ? `${pngEncode.ms.toFixed(0)} ms, ${pngEncode.bytes} bytes` : "not recorded"} (not a target)`,
    );
    report(
      `Ctrl+C (WebDriver send) → 11520x2160 image on the clipboard = ${clipboardMs.toFixed(0)} ms (not a target; ` +
        `includes Rust copy_png in the debug build and the clipboard read)`,
    );
    expect({ width: copied.width, height: copied.height }).toEqual({ width: 11520, height: 2160 });
    expect(
      compositeMs,
      `composite ${compositeMs.toFixed(0)} ms, over by ${(compositeMs - TARGET.compositeMs).toFixed(0)} ms`,
    ).toBeLessThanOrEqual(TARGET.compositeMs);
  });

  it("表示中の画像の上で図形をドラッグしても 60fps 付近で描画される (11.1)", async () => {
    await focusEditor(label, title);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));
    await startFrameCount();
    await pressButton(browser, "四角 (R)");

    const from = at(box, 0.2, 0.6);
    const to = at(box, 0.8, 0.85);
    const t0 = await browser.execute(() => {
      const w = window as unknown as PerfWindow;
      w.perfFrames = [];
      w.perfCounting = true;
      return performance.now();
    });
    await slowDrag(from, to);
    const t1 = await browser.execute(() => performance.now());
    const frames = await browser.execute(() => {
      const w = window as unknown as PerfWindow;
      w.perfCounting = false;
      return w.perfFrames;
    });
    // The drag must have drawn a shape, or the frames only show an idle page.
    await waitUntil(
      "the dragged rectangle saved",
      async () => savedDocument(session.workspace.saveDir, WIDE.id)?.shapes.length === 4,
    );

    const during = frames.filter((t) => t >= t0 && t <= t1);
    const seconds = (t1 - t0) / 1000;
    const fps = during.length / seconds;
    let worstGap = 0;
    for (let i = 1; i < during.length; i++) {
      worstGap = Math.max(worstGap, during[i]! - during[i - 1]!);
    }
    report(
      `drag over the image = ${fps.toFixed(1)} fps (${during.length} frames in ${(seconds * 1000).toFixed(0)} ms; ` +
        `worst frame gap ${worstGap.toFixed(1)} ms; target ≈ ${TARGET.fps} fps, asserted ≥ ${TARGET.fpsFloor})`,
    );
    expect(
      fps,
      `${fps.toFixed(1)} fps, under by ${(TARGET.fpsFloor - fps).toFixed(1)} fps`,
    ).toBeGreaterThanOrEqual(TARGET.fpsFloor);
  });
});
