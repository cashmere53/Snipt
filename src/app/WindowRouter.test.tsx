import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import { WindowRouter } from "./WindowRouter";

// A window's feature is imported on first render. Under parallel load on Windows (`mise run check`)
// that import, and the editor's first canvas, can take longer than findBy's default 1000 ms.
const RENDER_TIMEOUT = 30_000;
const TEST_TIMEOUT = 2 * RENDER_TIMEOUT;

describe("WindowRouter", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);
  // Windows load their data on mount; only the history list needs an answer here.
  // The editor route subscribes to real window events, and their cleanup runs a tick after the
  // unmount: let it run before the setup file clears the IPC mock.
  afterEach(async () => {
    cleanup();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  beforeEach(() =>
    mockIPC(
      (cmd) => (cmd === "library_list" ? [] : Promise.reject(new Error(`${cmd}: not mocked`))),
      {
        shouldMockEvents: true,
      },
    ),
  );

  it.each([
    ["settings", "SettingsWindow"],
    ["history", "HistoryWindow"],
    ["editor-abc", "EditorWindow"],
    ["overlay-1-1", "OverlayWindow"],
  ])(
    "renders the %s window",
    async (label, testId) => {
      mockWindows(label);
      render(<WindowRouter label={label} />);
      expect(
        await screen.findByTestId(testId, undefined, { timeout: RENDER_TIMEOUT }),
      ).toBeInTheDocument();
    },
    TEST_TIMEOUT,
  );

  it(
    "hands the parsed parameters to the window",
    async () => {
      mockWindows("editor-abc");
      render(<WindowRouter label="editor-abc" />);
      expect(
        await screen.findByTestId("EditorWindow", undefined, { timeout: RENDER_TIMEOUT }),
      ).toHaveTextContent("abc");
    },
    TEST_TIMEOUT,
  );

  it("renders nothing and logs for an unknown label", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const { container } = render(<WindowRouter label="main" />);
    expect(container).toBeEmptyDOMElement();
    expect(error).toHaveBeenCalledWith("Unknown window label: main");
  });
});
