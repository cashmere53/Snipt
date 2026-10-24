import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import Konva from "konva";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import { grayImage, ORIGIN, useViewSize } from "./canvas/testing";
import { EditorWindow, type EditorWindowDeps } from "./EditorWindow";
import { CURRENT_SCHEMA_VERSION, type Point } from "./model/document";
import { DEFAULT_STYLE } from "./model/style";
import { rect } from "./model/testing";
import { STYLE_PREFS_KEY } from "./services/style-prefs";
import { fail, fakeIpc, fakePort, ok, sessionDto } from "./services/testing";

const stored = {
  schemaVersion: CURRENT_SCHEMA_VERSION,
  image: { width: 400, height: 300 },
  crop: null,
  shapes: [rect("r")],
};

function setup(ipc = fakeIpc()) {
  const window = fakePort();
  let n = 0;
  const deps: EditorWindowDeps = {
    ipc,
    windowPort: () => window.port,
    decode: async () => grayImage(),
    newId: () => `new-${++n}`,
  };
  const result = render(<EditorWindow captureId="20261022_174150" deps={deps} />);
  return { ...result, ipc, window };
}

/** A click on the canvas at image point `at`. */
function click(at: Point) {
  const stage = Konva.stages.at(-1)!;
  for (const type of ["mousedown", "mouseup"]) {
    act(() => {
      stage.draw();
      stage.content.dispatchEvent(
        new MouseEvent(type, { bubbles: true, clientX: ORIGIN.x + at.x, clientY: ORIGIN.y + at.y }),
      );
    });
  }
}

/** The window is loaded and its stage has the view's size. */
async function ready() {
  await screen.findByRole("toolbar", { name: "ツール" });
  await waitFor(() => expect(Konva.stages.at(-1)?.width()).toBe(800));
}

describe("EditorWindow", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);
  useViewSize();
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.restoreAllMocks());

  it("opens the original with its annotations at the initial zoom", async () => {
    setup(fakeIpc({ openSession: async () => ok(sessionDto({ annotations: stored })) }));
    await ready();
    expect(screen.getByLabelText("表示倍率")).toHaveTextContent("100%");
    expect(Konva.stages.at(-1)!.findOne("#r")).toBeDefined();
    expect(screen.queryByText(/元画像のみで開きました/)).toBeNull();
  });

  it("shows why the annotations were not loaded, and still edits the original", async () => {
    setup(fakeIpc({ openSession: async () => ok(sessionDto({ annotations: { bad: 1 } })) }));
    await ready();
    expect(screen.getByText(/元画像のみで開きました/)).toBeInTheDocument();
  });

  it("shows only the reason and a close button when the capture cannot be opened", async () => {
    const f = setup(fakeIpc({ openSession: async () => fail("notFound", "見つかりません") }));
    expect(await screen.findByText("見つかりません")).toBeInTheDocument();
    expect(screen.queryByRole("toolbar")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "閉じる" }));
    expect(f.window.port.destroy).toHaveBeenCalledOnce();
  });

  it("starts with the last style and remembers a new one", async () => {
    localStorage.setItem(STYLE_PREFS_KEY, JSON.stringify({ ...DEFAULT_STYLE, strokeWidth: 8 }));
    setup();
    await ready();
    expect(screen.getByLabelText("線の太さ")).toHaveValue("8");
    fireEvent.change(screen.getByLabelText("線の太さ"), { target: { value: "12" } });
    expect(JSON.parse(localStorage.getItem(STYLE_PREFS_KEY)!)).toMatchObject({ strokeWidth: 12 });
  });

  it("saves a second after a change", async () => {
    const f = setup();
    await ready();
    fireEvent.keyDown(window, { key: "n", code: "KeyN" });
    click({ x: 50, y: 50 });
    expect(Konva.stages.at(-1)!.find(".shape")).toHaveLength(1);
    expect(f.ipc.save).not.toHaveBeenCalled();
    await waitFor(() => expect(f.ipc.save).toHaveBeenCalledOnce(), { timeout: 3_000 });
    expect(await screen.findByText("保存済み")).toBeInTheDocument();
  });

  it("copies and saves as from the toolbar and the keys, showing failures", async () => {
    const f = setup(fakeIpc({ copy: async () => fail("clipboard", "クリップボードを使えません") }));
    await ready();
    fireEvent.click(screen.getByTitle("コピー (Ctrl+C)"));
    expect(await screen.findByRole("alert")).toHaveTextContent("クリップボードを使えません");
    const [png] = f.ipc.copy.mock.calls[0]!;
    expect([...png.subarray(1, 4)]).toEqual([0x50, 0x4e, 0x47]);

    fireEvent.keyDown(window, { key: "S", code: "KeyS", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(f.ipc.writeExport).toHaveBeenCalledOnce());
    expect(f.ipc.pickExportPath).toHaveBeenCalledWith("Snipt_20261022_174150_export.png");
  });

  it("saves before the user's close and closes", async () => {
    const f = setup();
    await ready();
    await waitFor(() => expect(f.window.listening()).toBe(true));
    fireEvent.keyDown(window, { key: "n", code: "KeyN" });
    click({ x: 50, y: 50 });
    expect(f.window.userClose()).toBe(true);
    await waitFor(() => expect(f.window.port.destroy).toHaveBeenCalledOnce());
    expect(f.ipc.save).toHaveBeenCalledOnce();
  });

  it("asks to retry or discard when the save before closing fails", async () => {
    const f = setup(fakeIpc({ save: async () => fail("io", "書き込めません") }));
    await ready();
    await waitFor(() => expect(f.window.listening()).toBe(true));
    fireEvent.keyDown(window, { key: "n", code: "KeyN" });
    click({ x: 50, y: 50 });
    f.window.userClose();
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("書き込めません");
    fireEvent.click(screen.getByRole("button", { name: "変更を破棄して閉じる" }));
    await waitFor(() => expect(f.window.port.destroy).toHaveBeenCalledOnce());
  });
});
