// Snipt's windows as WebDriver sees them: every webview is a window handle, told apart by its
// Tauri label (`history`, `overlay-*`, `editor-<id>`).
import { readFileSync } from "node:fs";
import { join } from "node:path";

type Browser = WebdriverIO.Browser;

export interface Point {
  x: number;
  y: number;
}

/** The image as shown on the editor canvas, in CSS pixels of the viewport. */
export interface ImageBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

export async function waitUntil<T>(
  what: string,
  probe: () => Promise<T | null | undefined | false>,
  timeoutMs = 15_000,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let last: unknown;
  for (;;) {
    try {
      const value = await probe();
      if (value !== null && value !== undefined && value !== false) {
        return value;
      }
    } catch (e) {
      last = e;
    }
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for ${what}${last ? `: ${String(last)}` : ""}`);
    }
    await new Promise((r) => setTimeout(r, 200));
  }
}

async function currentLabel(browser: Browser): Promise<string | null> {
  return browser.execute(() => {
    const internals = Reflect.get(window, "__TAURI_INTERNALS__") as
      | { metadata?: { currentWindow?: { label?: string } } }
      | undefined;
    return internals?.metadata?.currentWindow?.label ?? null;
  });
}

// A handle keeps its webview, so each handle is asked for its label once: switching into every
// window on each poll is slow and can stall on a webview that is still loading.
const labelOfHandle = new WeakMap<Browser, Map<string, string>>();

/** Label → window handle for every webview open now. */
export async function windowsByLabel(browser: Browser): Promise<Map<string, string>> {
  const known = labelOfHandle.get(browser) ?? new Map<string, string>();
  labelOfHandle.set(browser, known);
  const labels = new Map<string, string>();
  for (const handle of await browser.getWindowHandles()) {
    let label = known.get(handle) ?? null;
    if (label === null) {
      try {
        await browser.switchToWindow(handle);
        label = await currentLabel(browser);
      } catch {
        // closed or not ready yet: asked again on the next call
      }
      if (label !== null) {
        known.set(handle, label);
      }
    }
    if (label !== null) {
      labels.set(label, handle);
    }
  }
  return labels;
}

export async function editorLabels(browser: Browser): Promise<string[]> {
  return [...(await windowsByLabel(browser)).keys()]
    .filter((l) => l.startsWith("editor-"))
    .toSorted();
}

export async function switchTo(browser: Browser, label: string): Promise<void> {
  const handle = await waitUntil(`window ${label}`, async () =>
    (await windowsByLabel(browser)).get(label),
  );
  await browser.switchToWindow(handle);
}

/** The new editor label once one appears that is not in `before`. */
export async function waitForNewEditor(
  browser: Browser,
  before: readonly string[],
  timeoutMs?: number,
): Promise<string> {
  return waitUntil(
    "a new editor window",
    async () => (await editorLabels(browser)).find((l) => !before.includes(l)),
    timeoutMs,
  );
}

/** The window title as Tauri set it (the editor's is `Snipt - YYYY/MM/DD HH:MM:SS`). */
export async function windowTitle(browser: Browser): Promise<string> {
  return browser.execute(async () => {
    type Internals = {
      invoke(cmd: string, args: unknown): Promise<string>;
      metadata: { currentWindow: { label: string } };
    };
    const internals = Reflect.get(window, "__TAURI_INTERNALS__") as Internals;
    return internals.invoke("plugin:window|title", {
      label: internals.metadata.currentWindow.label,
    });
  });
}

export async function openFromHistory(browser: Browser, ariaLabel: string): Promise<void> {
  await switchTo(browser, "history");
  const tile = await browser.$(`[role="option"][aria-label="${ariaLabel}"]`);
  await tile.waitForExist({ timeout: 15_000 });
  await tile.doubleClick();
}

/**
 * Where the original image is drawn: the bounding box of the opaque pixels on the first Konva
 * layer (the image layer). `null` until the image is on the canvas.
 */
export async function imageBox(browser: Browser): Promise<ImageBox | null> {
  return browser.execute(() => {
    const canvas = document.querySelector<HTMLCanvasElement>(".konvajs-content canvas");
    const context = canvas?.getContext("2d");
    if (!canvas || !context || canvas.width === 0) {
      return null;
    }
    const { data, width, height } = context.getImageData(0, 0, canvas.width, canvas.height);
    let minX = width;
    let minY = height;
    let maxX = -1;
    let maxY = -1;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        if ((data[(y * width + x) * 4 + 3] ?? 0) > 0) {
          minX = Math.min(minX, x);
          maxX = Math.max(maxX, x);
          minY = Math.min(minY, y);
          maxY = Math.max(maxY, y);
        }
      }
    }
    if (maxX < 0) {
      return null;
    }
    const rect = canvas.getBoundingClientRect();
    const scale = rect.width / canvas.width;
    return {
      left: rect.left + minX * scale,
      top: rect.top + minY * scale,
      width: (maxX - minX + 1) * scale,
      height: (maxY - minY + 1) * scale,
    };
  });
}

/** True once any layer above the image layer has something drawn (the annotations). */
export async function hasAnnotationsDrawn(browser: Browser): Promise<boolean> {
  return browser.execute(() => {
    const canvases = [...document.querySelectorAll<HTMLCanvasElement>(".konvajs-content canvas")];
    return canvases.slice(1).some((canvas) => {
      const context = canvas.getContext("2d");
      if (!context || canvas.width === 0) {
        return false;
      }
      const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
      for (let i = 3; i < data.length; i += 4) {
        if ((data[i] ?? 0) > 0) {
          return true;
        }
      }
      return false;
    });
  });
}

/** A point given as fractions of the shown image, in viewport CSS pixels. */
export function at(box: ImageBox, fx: number, fy: number): Point {
  return { x: Math.round(box.left + box.width * fx), y: Math.round(box.top + box.height * fy) };
}

export async function drag(browser: Browser, from: Point, to: Point): Promise<void> {
  await browser
    .action("pointer", { parameters: { pointerType: "mouse" } })
    .move({ x: from.x, y: from.y, origin: "viewport" })
    .down({ button: 0 })
    .move({
      x: Math.round((from.x + to.x) / 2),
      y: Math.round((from.y + to.y) / 2),
      duration: 50,
      origin: "viewport",
    })
    .move({ x: to.x, y: to.y, duration: 50, origin: "viewport" })
    .up({ button: 0 })
    .perform();
}

export async function click(browser: Browser, point: Point): Promise<void> {
  await browser
    .action("pointer", { parameters: { pointerType: "mouse" } })
    .move({ x: point.x, y: point.y, origin: "viewport" })
    .down({ button: 0 })
    .up({ button: 0 })
    .perform();
}

/** Toolbar buttons are titled `<name> (<key>)`, e.g. `矢印 (A)`. */
export async function pressButton(browser: Browser, title: string): Promise<void> {
  await (await browser.$(`button[title="${title}"]`)).click();
}

export async function pressCtrl(browser: Browser, key: string): Promise<void> {
  await browser.action("key").down("").down(key).up(key).up("").perform();
}

export interface SavedRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SavedDocument {
  image: { width: number; height: number };
  crop: SavedRect | null;
  shapes: ({ kind: string; rect?: SavedRect } & Record<string, unknown>)[];
}

/** `<save folder>/.snipt/annotations/<id>.json`, or `null` before the first save. */
export function savedDocument(saveDir: string, id: string): SavedDocument | null {
  try {
    return JSON.parse(
      readFileSync(join(saveDir, ".snipt", "annotations", `${id}.json`), "utf8"),
    ) as SavedDocument;
  } catch {
    return null;
  }
}

/**
 * Reads the annotations file every few milliseconds and keeps the last document seen, and when
 * the file was first seen. A file that is written and then moved away (to the recycle bin) is
 * still caught.
 */
export function watchAnnotations(
  saveDir: string,
  id: string,
): { latest(): SavedDocument | null; firstSeenAt(): number | null; stop(): void } {
  let latest: SavedDocument | null = null;
  let firstSeen: number | null = null;
  const timer = setInterval(() => {
    const doc = savedDocument(saveDir, id);
    if (doc !== null && firstSeen === null) {
      firstSeen = Date.now();
    }
    latest = doc ?? latest;
  }, 5);
  return {
    latest: () => latest,
    firstSeenAt: () => firstSeen,
    stop: () => clearInterval(timer),
  };
}
