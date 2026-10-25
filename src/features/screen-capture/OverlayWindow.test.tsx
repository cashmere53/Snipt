import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import { OverlayWindow } from "./OverlayWindow";
import { activate, fakeDeps } from "./services/testing";

async function mounted() {
  const fake = fakeDeps();
  render(<OverlayWindow monitorIndex={1} deps={fake.deps} />);
  // Listening first, then ready.
  await waitFor(() => expect(fake.ipc.ready).toHaveBeenCalledTimes(1));
  return fake;
}

const mode = () => screen.getByTestId("OverlayWindow").dataset.mode;

describe("OverlayWindow", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);

  it("waits empty and transparent until a session is activated", async () => {
    await mounted();
    expect(mode()).toBe("idle");
    expect(screen.getByTestId("OverlayWindow")).toBeEmptyDOMElement();
    expect(document.documentElement).toHaveClass("overlay-page");
  });

  it("switches to the session's mode and reports it shown after the next frame", async () => {
    const { ipc, events, frames } = await mounted();
    act(() => events.activate(activate(4, "region")));
    expect(mode()).toBe("region");
    expect(screen.getByTestId("region-selector")).toBeInTheDocument();
    expect(ipc.shown).not.toHaveBeenCalled();
    act(() => frames.run());
    expect(ipc.shown).toHaveBeenCalledWith(4, 1);

    act(() => events.activate(activate(5, "window")));
    expect(mode()).toBe("window");
    expect(screen.getByTestId("window-highlighter")).toBeInTheDocument();
  });

  it("cancels on Esc and on a right click", async () => {
    const { ipc, events } = await mounted();
    act(() => events.activate(activate(4, "region")));
    fireEvent.keyDown(window, { key: "Escape" });
    const menu = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    act(() => {
      screen.getByTestId("region-selector").dispatchEvent(menu);
    });
    expect(menu.defaultPrevented).toBe(true);
    expect(ipc.cancel.mock.calls).toEqual([[4], [4]]);
    // Other keys do nothing.
    fireEvent.keyDown(window, { key: "a" });
    expect(ipc.cancel).toHaveBeenCalledTimes(2);
  });

  it("drops the session on deactivate and ignores earlier sessions' events", async () => {
    const { ipc, events } = await mounted();
    act(() => events.activate(activate(4, "window")));
    act(() => events.activate(activate(3, "region")));
    expect(mode()).toBe("window");
    act(() => events.highlight({ sessionId: 3, rect: { x: 1920, y: 0, width: 100, height: 100 } }));
    expect(screen.queryByTestId("highlight")).toBeNull();
    act(() => events.deactivate({ sessionId: 3 }));
    expect(mode()).toBe("window");
    act(() => events.deactivate({ sessionId: 4 }));
    expect(mode()).toBe("idle");
    // Nothing cancels without a session.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(ipc.cancel).not.toHaveBeenCalled();
  });
});
