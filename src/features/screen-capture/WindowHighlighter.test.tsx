import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WindowHighlighter } from "./WindowHighlighter";
import { activate, fail, fakeFrames, fakeIpc } from "./services/testing";

const session = activate(9, "window");
const surface = () => screen.getByTestId("window-highlighter");

describe("WindowHighlighter", () => {
  it("sends the pointer once per frame, in physical pixels", () => {
    const ipc = fakeIpc();
    const frames = fakeFrames();
    render(
      <WindowHighlighter
        session={session}
        highlight={null}
        ipc={ipc}
        nextFrame={frames.nextFrame}
      />,
    );
    fireEvent.pointerMove(surface(), { clientX: 10, clientY: 10 });
    fireEvent.pointerMove(surface(), { clientX: 20, clientY: 30 });
    act(() => frames.run());
    expect(ipc.pointer.mock.calls).toEqual([[9, 1, 30, 45]]);
  });

  it("frames only the part of the window on this monitor", () => {
    const ipc = fakeIpc();
    // From the primary monitor across into this one (which starts at x = 1920).
    const highlight = { x: 1800, y: 30, width: 300, height: 150 };
    render(
      <WindowHighlighter
        session={session}
        highlight={highlight}
        ipc={ipc}
        nextFrame={fakeFrames().nextFrame}
      />,
    );
    const frame = screen.getByTestId("highlight");
    expect(frame.style.left).toBe("0px");
    expect(frame.style.top).toBe("20px");
    expect(frame.style.width).toBe("120px");
    expect(frame.style.height).toBe("100px");
  });

  it("draws nothing for a window on another monitor", () => {
    render(
      <WindowHighlighter
        session={session}
        highlight={{ x: 0, y: 0, width: 800, height: 600 }}
        ipc={fakeIpc()}
        nextFrame={fakeFrames().nextFrame}
      />,
    );
    expect(screen.queryByTestId("highlight")).toBeNull();
  });

  it("hides the cursor before committing, and shows it again when nothing is highlighted", async () => {
    const ipc = fakeIpc();
    let cursorAtCommit = "";
    ipc.commitWindow.mockImplementation(async () => {
      cursorAtCommit = surface().style.cursor;
      return fail("noWindowHighlighted");
    });
    render(
      <WindowHighlighter
        session={session}
        highlight={null}
        ipc={ipc}
        nextFrame={fakeFrames().nextFrame}
      />,
    );
    await act(async () => fireEvent.click(surface()));
    expect(ipc.commitWindow).toHaveBeenCalledWith(9);
    expect(cursorAtCommit).toBe("none");
    expect(surface().style.cursor).toBe("");
  });
});
