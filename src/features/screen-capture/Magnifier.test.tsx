import { act, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import { MAGNIFIER_RADIUS, MAGNIFIER_ZOOM, Magnifier } from "./Magnifier";
import { activate, fakeFrames, fakeIpc, ok } from "./services/testing";

const { monitor } = activate(7, "region");
const SIDE = 2 * MAGNIFIER_RADIUS + 1;

function view(
  ipc: ReturnType<typeof fakeIpc>,
  frames: ReturnType<typeof fakeFrames>,
  x: number,
  y: number,
) {
  return (
    <Magnifier
      sessionId={7}
      monitor={monitor}
      point={{ x, y }}
      ipc={ipc}
      nextFrame={frames.nextFrame}
    />
  );
}

describe("Magnifier", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);

  it("asks once per frame, for the latest point", () => {
    const ipc = fakeIpc();
    const frames = fakeFrames();
    const { rerender } = render(view(ipc, frames, 10, 10));
    rerender(view(ipc, frames, 11, 10));
    rerender(view(ipc, frames, 12, 13));
    expect(frames.pending()).toBe(1);
    expect(ipc.magnifier).not.toHaveBeenCalled();
    act(() => frames.run());
    expect(ipc.magnifier.mock.calls).toEqual([[7, 1, 12, 13, MAGNIFIER_RADIUS]]);
  });

  it("shows the global physical coordinates", () => {
    render(view(fakeIpc(), fakeFrames(), 12, 13));
    expect(screen.getByText("X: 1932 Y: 13")).toBeInTheDocument();
  });

  it("draws the returned pixels enlarged", async () => {
    const ipc = fakeIpc();
    // Every pixel red, the center one blue.
    const rgba = new Uint8Array(SIDE * SIDE * 4);
    for (let i = 0; i < SIDE * SIDE; i++) rgba.set([255, 0, 0, 255], i * 4);
    rgba.set([0, 0, 255, 255], (MAGNIFIER_RADIUS * SIDE + MAGNIFIER_RADIUS) * 4);
    ipc.magnifier.mockResolvedValue(ok(rgba));
    const frames = fakeFrames();
    render(view(ipc, frames, 10, 10));
    await act(async () => frames.run());
    const canvas = screen.getByTestId("magnifier").querySelector("canvas")!;
    const context = canvas.getContext("2d")!;
    const at = (x: number, y: number) => Array.from(context.getImageData(x, y, 1, 1).data);
    expect(at(2, 2)).toEqual([255, 0, 0, 255]);
    const center = MAGNIFIER_RADIUS * MAGNIFIER_ZOOM + MAGNIFIER_ZOOM / 2;
    expect(at(center, center)).toEqual([0, 0, 255, 255]);
  });
});
