// annotation-editor 7.1: the editor's main scenarios on a real Windows desktop.
// Requirements 1.1, 1.4, 1.5 (opening), 10.1, 10.3 (copy of the cropped composite), 11.2 (flush
// on close). The scenarios run in order on one app session.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  at,
  click,
  drag,
  editorLabels,
  hasAnnotationsDrawn,
  imageBox,
  openFromHistory,
  pressButton,
  pressCtrl,
  savedDocument,
  switchTo,
  waitForNewEditor,
  waitUntil,
  windowTitle,
} from "../support/app";
import { Desktop } from "../support/desktop";
import { HOTKEYS, type Session, startSession } from "../support/driver";
import { editorTitle, type SeedCapture } from "../support/png";

const GRAY: SeedCapture = {
  id: "20200102_030405",
  capturedAt: "2020-01-02T03:04:05+09:00",
  width: 800,
  height: 600,
  rgba: [128, 128, 128, 255],
};
const LIGHT: SeedCapture = {
  id: "20200102_030406",
  capturedAt: "2020-01-02T03:04:06+09:00",
  width: 640,
  height: 480,
  rgba: [200, 200, 200, 255],
};
const HISTORY_TITLE = "Snipt 履歴";

/** The history tile's accessible name: `yyyy/MM/dd HH:mm:ss 幅 × 高さ`. */
function tileLabel(capture: SeedCapture): string {
  const [date, time] = capture.capturedAt.slice(0, 19).split("T") as [string, string];
  return `${date.replaceAll("-", "/")} ${time} ${capture.width} × ${capture.height}`;
}

let session: Session;
let desktop: Desktop;
let browser: WebdriverIO.Browser;

async function waitForForeground(title: string): Promise<void> {
  let last = "";
  await waitUntil(`"${title}" in the foreground`, async () => {
    last = await desktop.foregroundTitle();
    return last === title;
  }).catch((e: unknown) => {
    throw new Error(`${String(e)} (foreground: "${last}")`);
  });
}

async function historyTiles(): Promise<string[]> {
  await switchTo(browser, "history");
  return browser.execute(() =>
    [...document.querySelectorAll('[role="option"]')].map(
      (t) => t.getAttribute("aria-label") ?? "",
    ),
  );
}

beforeAll(async () => {
  session = await startSession([GRAY, LIGHT]);
  browser = session.browser;
  desktop = new Desktop();
  await waitUntil(
    "the seeded captures in the history",
    async () => (await historyTiles()).length === 2,
  );
});

afterAll(async () => {
  desktop?.dispose();
  await session?.close();
});

describe("annotation-editor E2E", () => {
  let captured: { label: string; title: string };

  it("撮影すると編集画面が前面に開き、履歴の先頭に追加される (1.1)", async () => {
    // Start from a Snipt window in the foreground: with another app there, Windows' foreground
    // lock decides whether the editor may take focus (see Implementation Notes 7.1).
    await desktop.activate(HISTORY_TITLE);
    await waitForForeground(HISTORY_TITLE);
    const before = await editorLabels(browser);

    await desktop.pressKeys(HOTKEYS.fullscreen);

    // Capturing, saving and creating the window take a few seconds in the debug build.
    const label = await waitForNewEditor(browser, before, 30_000);
    await switchTo(browser, label);
    const title = await windowTitle(browser);
    expect(title).toMatch(/^Snipt - \d{4}\/\d{2}\/\d{2} \d{2}:\d{2}:\d{2}/);
    await waitForForeground(title);
    captured = { label, title };

    const tiles = await waitUntil("the new capture in the history", async () => {
      const labels = await historyTiles();
      return labels.length === 3 ? labels : null;
    });
    expect([tileLabel(GRAY), tileLabel(LIGHT)]).not.toContain(tiles[0]);
  });

  it("同じキャプチャを履歴から開くと既存の画面が前面に来て、別のキャプチャは別の画面で開く (1.4, 1.5)", async () => {
    await desktop.activate(HISTORY_TITLE);
    await waitForForeground(HISTORY_TITLE);
    const before = await editorLabels(browser);

    await switchTo(browser, "history");
    const [newest] = await browser.$$('[role="option"]');
    await newest!.doubleClick();

    await waitForForeground(captured.title);
    expect(await editorLabels(browser)).toEqual(before);

    await openFromHistory(browser, tileLabel(GRAY));
    const label = await waitForNewEditor(browser, before);
    expect(label).toBe(`editor-${GRAY.id}`);
    await waitForForeground(editorTitle(GRAY.id));
    expect(await editorLabels(browser)).toHaveLength(before.length + 1);
  });

  it("矢印・四角・番号・ぼかし・切り抜きの後に Ctrl+C すると、切り抜き範囲と同じ大きさの画像がコピーされる (10.1, 10.3)", async () => {
    await switchTo(browser, `editor-${GRAY.id}`);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));

    await pressButton(browser, "矢印 (A)");
    await drag(browser, at(box, 0.2, 0.2), at(box, 0.4, 0.3));
    await pressButton(browser, "四角 (R)");
    await drag(browser, at(box, 0.42, 0.2), at(box, 0.55, 0.35));
    await pressButton(browser, "番号 (N)");
    await click(browser, at(box, 0.3, 0.5));
    await pressButton(browser, "ぼかし (B)");
    await drag(browser, at(box, 0.15, 0.5), at(box, 0.25, 0.6));
    await pressButton(browser, "切り抜き (C)");
    await drag(browser, at(box, 0.1, 0.1), at(box, 0.6, 0.7));
    await pressButton(browser, "切り抜きを確定 (Enter)");

    await desktop.clearClipboard();
    await pressCtrl(browser, "c");
    const copied = await waitUntil("an image on the clipboard", () => desktop.clipboardImage());

    // Autosave writes the document a second after the last change.
    const doc = await waitUntil("the saved annotations", async () => {
      const saved = savedDocument(session.workspace.saveDir, GRAY.id);
      return saved?.crop && saved.shapes.length === 4 ? saved : null;
    });
    expect(doc.shapes.map((s) => s.kind).toSorted()).toEqual(["arrow", "marker", "mosaic", "rect"]);
    const crop = doc.crop!;
    expect(crop.width).toBeLessThan(GRAY.width);
    expect(crop.height).toBeLessThan(GRAY.height);
    expect({ width: copied.width, height: copied.height }).toEqual({
      width: crop.width,
      height: crop.height,
    });
  });

  it("編集してすぐ閉じても、履歴から開き直すと注釈が復元されている (11.2)", async () => {
    const title = editorTitle(LIGHT.id);
    const before = await editorLabels(browser);
    await openFromHistory(browser, tileLabel(LIGHT));
    const label = await waitForNewEditor(browser, before);
    await switchTo(browser, label);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));

    await pressButton(browser, "四角 (R)");
    await drag(browser, at(box, 0.3, 0.3), at(box, 0.7, 0.7));
    const drawnAt = Date.now();
    await desktop.close(title);
    // Closed before the 1 second autosave could run: only the flush on close can save it.
    expect(Date.now() - drawnAt).toBeLessThan(1000);

    await waitUntil("the editor to close", async () => !(await desktop.windowExists(title)));
    const doc = savedDocument(session.workspace.saveDir, LIGHT.id);
    expect(doc?.shapes.map((s) => s.kind)).toEqual(["rect"]);
    const rect = doc!.shapes[0]!.rect!;

    const remaining = await editorLabels(browser);
    expect(remaining).not.toContain(label);
    await openFromHistory(browser, tileLabel(LIGHT));
    expect(await waitForNewEditor(browser, remaining)).toBe(label);
    await switchTo(browser, label);
    await waitUntil("the restored annotations on the canvas", () => hasAnnotationsDrawn(browser));

    // The composite has the rectangle's stroke on its top edge (the default red, #E53935).
    const x = Math.round(rect.x + rect.width / 2);
    const points: [number, number][] = [-2, -1, 0, 1, 2].map((dy) => [x, rect.y + dy]);
    await desktop.clearClipboard();
    await pressCtrl(browser, "c");
    const copied = await waitUntil("an image on the clipboard", () =>
      desktop.clipboardImage(points),
    );
    expect({ width: copied.width, height: copied.height }).toEqual({
      width: LIGHT.width,
      height: LIGHT.height,
    });
    const red = copied.pixels.some(([r, g, b]) => r > 180 && g < 120 && b < 120);
    expect(red, `stroke pixels: ${JSON.stringify(copied.pixels)}`).toBe(true);
  });
});
