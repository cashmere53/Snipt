// annotation-editor 7.2: Japanese input, closing before a delete, and the flush on quit, on a real
// Windows desktop. Requirements 5.1, 5.4 (text tool, IME), 11.2 (flush before the window closes
// for a delete, and on quit), 11.3 (retry at quit). The scenarios run in order on one app session;
// the quit scenario is last because it ends the app.
import { mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  at,
  click,
  drag,
  editorLabels,
  imageBox,
  openFromHistory,
  pressButton,
  savedDocument,
  switchTo,
  waitForNewEditor,
  waitUntil,
  watchAnnotations,
} from "../support/app";
import { Desktop } from "../support/desktop";
import { appRunning, requestQuit, type Session, sendCdp, startSession } from "../support/driver";
import { editorTitle, type SeedCapture } from "../support/png";

const TEXT: SeedCapture = {
  id: "20200102_030407",
  capturedAt: "2020-01-02T03:04:07+09:00",
  width: 640,
  height: 480,
  rgba: [200, 200, 200, 255],
};
const DELETED: SeedCapture = {
  ...TEXT,
  id: "20200102_030408",
  capturedAt: "2020-01-02T03:04:08+09:00",
};
const QUITTING: SeedCapture = {
  ...TEXT,
  id: "20200102_030409",
  capturedAt: "2020-01-02T03:04:09+09:00",
};
const EXIT_DIALOG = "Snipt を終了";

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

/** Brings the editor `label` (titled `title`) to the front and makes it the WebDriver target. */
async function focusEditor(label: string, title: string): Promise<void> {
  await switchTo(browser, label);
  await desktop.activate(title);
  await waitForForeground(title);
}

async function historyTiles(): Promise<string[]> {
  await switchTo(browser, "history");
  return browser.execute(() =>
    [...document.querySelectorAll('[role="option"]')].map(
      (t) => t.getAttribute("aria-label") ?? "",
    ),
  );
}

/** The editor's save status line (`保存済み`, `未保存の変更があります`, ...). */
function statusText(): Promise<string> {
  return browser.execute((): string => document.querySelector(".editor-status")?.textContent ?? "");
}

/** The text in the open text box, or `null` when there is none. */
function textBoxValue(): Promise<string | null> {
  return browser.execute(
    (): string | null =>
      document.querySelector<HTMLTextAreaElement>("textarea.editor-text-input")?.value ?? null,
  );
}

/** Opens the capture from the history and focuses its editor; returns the editor's label. */
async function openEditor(capture: SeedCapture): Promise<string> {
  const before = await editorLabels(browser);
  await openFromHistory(browser, tileLabel(capture));
  const label = await waitForNewEditor(browser, before);
  await focusEditor(label, editorTitle(capture.id));
  return label;
}

beforeAll(async () => {
  session = await startSession([TEXT, DELETED, QUITTING]);
  browser = session.browser;
  desktop = new Desktop();
  await waitUntil(
    "the seeded captures in the history",
    async () => (await historyTiles()).length === 3,
  );
});

afterAll(async () => {
  desktop?.dispose();
  await session?.close();
});

describe("annotation-editor flush E2E", () => {
  it("テキストツールで IME 変換中の Esc では確定されず、外側のクリックで確定される (5.1, 5.4)", async () => {
    await openEditor(TEXT);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));

    await pressButton(browser, "テキスト (T)");
    await click(browser, at(box, 0.3, 0.3));
    await waitUntil("the text box to have the focus", () =>
      browser.execute(
        (): boolean => document.activeElement?.classList.contains("editor-text-input") ?? false,
      ),
    );
    await browser.execute(() => {
      const log: string[] = [];
      Reflect.set(window, "__imeLog", log);
      for (const type of ["compositionstart", "compositionend"]) {
        document.addEventListener(type, () => log.push(type), true);
      }
      // Keys as the page receives them, before any handler: key, isComposing and keyCode.
      const keys: string[] = [];
      Reflect.set(window, "__keyLog", keys);
      document.addEventListener(
        "keydown",
        (e) => keys.push(`${e.key}|${e.isComposing}|${e.keyCode}`),
        true,
      );
    });

    // A real composition in the focused text box, as the Japanese IME makes it.
    await sendCdp(browser, "Input.imeSetComposition", {
      text: "にほん",
      selectionStart: 3,
      selectionEnd: 3,
    });
    await waitUntil(
      "the composition text in the box",
      async () => (await textBoxValue()) === "にほん",
    );
    expect(
      await browser.execute((): string[] => Reflect.get(window, "__imeLog") as string[]),
    ).toEqual(["compositionstart"]);

    // Esc during composition belongs to the IME: the box stays open with its text.
    const escape = {
      key: "Escape",
      code: "Escape",
      windowsVirtualKeyCode: 27,
      nativeVirtualKeyCode: 27,
    };
    await sendCdp(browser, "Input.dispatchKeyEvent", { type: "keyDown", ...escape });
    await sendCdp(browser, "Input.dispatchKeyEvent", { type: "keyUp", ...escape });
    // Nothing to wait for: the proof is that the box does not close within a short time.
    await new Promise((r) => setTimeout(r, 500));
    // The box staying open proves nothing if the key never arrived, so the page must have seen
    // the Escape during the composition first.
    const keys = await browser.execute((): string[] => Reflect.get(window, "__keyLog") as string[]);
    expect(
      keys.some((k) => k.startsWith("Escape|true|")),
      `an Escape keydown with isComposing=true reached the page (keys: ${keys.join(", ")})`,
    ).toBe(true);
    expect(await textBoxValue(), "the box after Esc during composition").toBe("にほん");
    expect(savedDocument(session.workspace.saveDir, TEXT.id)).toBeNull();

    // A click outside the box commits the text.
    const outside = await browser.execute(() => {
      const r = document.querySelector(".editor-canvas")!.getBoundingClientRect();
      return { x: Math.round(r.left + 8), y: Math.round(r.top + 8) };
    });
    expect(outside.x < box.left || outside.y < box.top, "a point outside the image").toBe(true);
    await click(browser, outside);

    await waitUntil("the text box to close", async () => (await textBoxValue()) === null);
    const doc = await waitUntil("the committed text saved", async () => {
      const saved = savedDocument(session.workspace.saveDir, TEXT.id);
      return saved?.shapes.length === 1 ? saved : null;
    });
    expect(doc.shapes).toEqual([expect.objectContaining({ kind: "text", text: "にほん" })]);
  });

  it("履歴から開いている画像を削除すると、未保存の注釈を保存してから編集画面が閉じる (11.2)", async () => {
    await openEditor(DELETED);
    const title = editorTitle(DELETED.id);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));
    const saveDir = session.workspace.saveDir;

    await pressButton(browser, "四角 (R)");
    await drag(browser, at(box, 0.3, 0.3), at(box, 0.7, 0.7));
    // The annotation is unsaved here: the autosave runs 1 second after the change, and the delete
    // must save it first. Every check below is made within that second, measured from here.
    const changedAt = Date.now();
    const sinceChange = () => Date.now() - changedAt;
    // The file is watched from now on, since the delete moves it to the recycle bin.
    const watch = watchAnnotations(saveDir, DELETED.id);
    try {
      await switchTo(browser, "history");
      await (await browser.$(`[role="option"][aria-label="${tileLabel(DELETED)}"]`)).click();
      await (await browser.$('//div[@role="toolbar"]//button[normalize-space()="削除"]')).click();
      const confirm = await browser.$('//button[normalize-space()="ごみ箱へ移動"]');
      await confirm.waitForExist({ timeout: 15_000 });
      expect(
        sinceChange(),
        "ms from the last change to the delete confirmation (a slower run cannot test the flush)",
      ).toBeLessThan(1000);
      expect(
        savedDocument(saveDir, DELETED.id),
        "no file yet: the autosave had not run before the delete",
      ).toBeNull();
      expect(
        sinceChange(),
        "ms from the last change to the check that the file is absent",
      ).toBeLessThan(1000);

      await confirm.click();
      // The flush writes the file. It must land within the same second as the change, or the
      // autosave, not the flush, is what saved it.
      const seenAt = await waitUntil("the flushed annotations file", async () =>
        watch.firstSeenAt(),
      );
      expect(
        seenAt - changedAt,
        "ms from the last change to the flushed file first seen (the flush must beat the autosave)",
      ).toBeLessThan(1000);
      await waitUntil("the editor to close", async () => !(await desktop.windowExists(title)));
      await waitUntil(
        "the capture to leave the history",
        async () => !(await historyTiles()).includes(tileLabel(DELETED)),
      );
    } finally {
      watch.stop();
    }

    expect(watch.latest()?.shapes).toEqual([expect.objectContaining({ kind: "rect" })]);
  });

  it("保存に失敗する状態でアプリを終了すると「注釈: <ID>」の確認が出て、再試行で保存されてから終了する (11.2, 11.3)", async () => {
    await openEditor(QUITTING);
    const box = await waitUntil("the image on the canvas", () => imageBox(browser));

    // A directory where the annotations file goes makes every save fail (the rename over it is
    // refused). The directory is removed again before the retry.
    const blocker = join(session.workspace.saveDir, ".snipt", "annotations", `${QUITTING.id}.json`);
    mkdirSync(blocker, { recursive: true });
    await pressButton(browser, "四角 (R)");
    await drag(browser, at(box, 0.3, 0.3), at(box, 0.7, 0.7));
    await waitUntil("the autosave to fail", async () =>
      (await statusText()).includes("保存できませんでした"),
    );

    requestQuit();
    const dialog = await waitUntil("the exit confirmation", async () => {
      const text = await desktop.windowText(EXIT_DIALOG);
      return text?.includes(`注釈: ${QUITTING.id}`) ? text : null;
    });
    expect(dialog).toContain("保存できていないデータが 1 件あります。");

    rmSync(blocker, { recursive: true, force: true });
    await waitUntil("the retry button", () => desktop.invokeButton(EXIT_DIALOG, "再試行"));
    await waitUntil("the app to exit", async () => !appRunning(), 60_000);

    const doc = savedDocument(session.workspace.saveDir, QUITTING.id);
    expect(doc?.shapes.map((s) => s.kind)).toEqual(["rect"]);
  });
});
