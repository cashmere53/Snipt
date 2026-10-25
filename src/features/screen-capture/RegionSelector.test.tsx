import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";
import { RegionSelector } from "./RegionSelector";
import { activate, fail, fakeFrames, fakeIpc } from "./services/testing";

function setup() {
  const ipc = fakeIpc();
  const frames = fakeFrames();
  render(<RegionSelector session={activate(7, "region")} ipc={ipc} nextFrame={frames.nextFrame} />);
  const surface = screen.getByTestId("region-selector");
  // CSS px; the monitor is at 150%.
  const drag = (from: [number, number], to: [number, number]) => {
    fireEvent.pointerDown(surface, { button: 0, pointerId: 1, clientX: from[0], clientY: from[1] });
    fireEvent.pointerMove(surface, { pointerId: 1, clientX: to[0], clientY: to[1] });
    fireEvent.pointerUp(surface, { button: 0, pointerId: 1, clientX: to[0], clientY: to[1] });
  };
  return { ipc, frames, surface, drag };
}

describe("RegionSelector", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);

  it("draws at the monitor's physical resolution", () => {
    setup();
    const canvas = screen.getByTestId("region-selector").querySelector("canvas");
    expect(canvas).toHaveAttribute("width", "400");
    expect(canvas).toHaveAttribute("height", "300");
    expect(canvas?.style.width).toBe(`${400 / 1.5}px`);
  });

  it("commits the dragged rectangle in physical pixels, in any direction", async () => {
    const { ipc, drag } = setup();
    await act(async () => drag([100, 80], [20, 10]));
    expect(ipc.commitRegion).toHaveBeenCalledWith(7, 1, { x: 30, y: 15, width: 120, height: 105 });
  });

  it("stops a drag past the monitor's edge at the edge and shows the size", () => {
    const { ipc, surface } = setup();
    fireEvent.pointerDown(surface, { button: 0, pointerId: 1, clientX: 200, clientY: 150 });
    fireEvent.pointerMove(surface, { pointerId: 1, clientX: 900, clientY: 900 });
    expect(screen.getByText("100 × 75")).toBeInTheDocument();
    fireEvent.pointerUp(surface, { button: 0, pointerId: 1, clientX: 900, clientY: 900 });
    expect(ipc.commitRegion).toHaveBeenCalledWith(7, 1, { x: 300, y: 225, width: 100, height: 75 });
  });

  it("does not commit less than a pixel and keeps selecting", async () => {
    const { ipc, drag } = setup();
    await act(async () => drag([50, 50], [50, 90]));
    expect(ipc.commitRegion).not.toHaveBeenCalled();
    await act(async () => drag([10, 10], [20, 20]));
    expect(ipc.commitRegion).toHaveBeenCalledTimes(1);
  });

  it("keeps selecting when Rust finds the selection too small", async () => {
    const { ipc, drag } = setup();
    ipc.commitRegion.mockResolvedValueOnce(fail("tooSmall"));
    await act(async () => drag([10, 10], [20, 20]));
    expect(screen.queryByText(/×/)).toBeNull();
    await act(async () => drag([10, 10], [30, 30]));
    expect(ipc.commitRegion).toHaveBeenCalledTimes(2);
  });

  it("ignores other buttons", () => {
    const { ipc, surface } = setup();
    fireEvent.pointerDown(surface, { button: 2, pointerId: 1, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(surface, { button: 2, pointerId: 1, clientX: 90, clientY: 90 });
    expect(ipc.commitRegion).not.toHaveBeenCalled();
  });
});
