import { describe, expect, it } from "vitest";
import {
  fitView,
  MAX_ZOOM,
  MIN_ZOOM,
  nextZoomStep,
  previousZoomStep,
  stageScale,
  zoomAround,
} from "./view";

const whole = (width: number, height: number) => ({ x: 0, y: 0, width, height });

describe("view", () => {
  it("shrinks large images to fit and centers them", () => {
    const view = fitView(whole(3840, 2160), { width: 1000, height: 600 }, 1);
    expect(view.zoom).toBeCloseTo(1000 / 3840);
    expect(view.mode).toBe("fit");
    const scale = stageScale(view.zoom, 1);
    expect(view.pan.x).toBeCloseTo(0);
    expect(view.pan.y).toBeCloseTo((600 - 2160 * scale) / 2);
  });

  it("shows small images at 100%, centered", () => {
    const view = fitView(whole(400, 300), { width: 1000, height: 600 }, 1);
    expect(view).toEqual({ zoom: 1, pan: { x: 300, y: 150 }, mode: "fit" });
  });

  it("maps one image pixel to one physical pixel at 100% on a 150% display", () => {
    // 1200 physical pixels fit in 800 CSS pixels at DPR 1.5.
    const view = fitView(whole(1200, 600), { width: 800, height: 600 }, 1.5);
    expect(view.zoom).toBe(1);
    const scale = stageScale(view.zoom, 1.5);
    expect(1200 * scale * 1.5).toBe(1200);
  });

  it("fits the crop, offset by its origin", () => {
    const view = fitView(
      { x: 100, y: 50, width: 200, height: 100 },
      { width: 400, height: 300 },
      1,
    );
    expect(view.zoom).toBe(1);
    // The crop's top left lands where a 200×100 image would be centered.
    expect(view.pan).toEqual({ x: 100 - 100, y: 100 - 50 });
  });

  it("keeps the zoom between 10% and 800%", () => {
    expect(fitView(whole(100_000, 100), { width: 100, height: 100 }, 1).zoom).toBe(MIN_ZOOM);
    const view = { zoom: 1, pan: { x: 0, y: 0 }, mode: "fit" as const };
    expect(zoomAround(view, 20, { x: 0, y: 0 }, 1).zoom).toBe(MAX_ZOOM);
    expect(zoomAround(view, 0.01, { x: 0, y: 0 }, 1).zoom).toBe(MIN_ZOOM);
  });

  it("zooms around the anchor", () => {
    const view = { zoom: 1, pan: { x: 10, y: 20 }, mode: "fit" as const };
    const zoomed = zoomAround(view, 2, { x: 110, y: 120 }, 2);
    // Image point (200, 200) was under the anchor before (scale 0.5) and stays there (scale 1).
    expect(zoomed).toEqual({ zoom: 2, pan: { x: -90, y: -80 }, mode: "manual" });
  });

  it("steps through the zoom levels", () => {
    expect(nextZoomStep(1)).toBe(1.25);
    expect(previousZoomStep(1)).toBe(0.75);
    expect(nextZoomStep(0.3)).toBe(0.33);
    expect(previousZoomStep(0.3)).toBe(0.25);
    expect(nextZoomStep(8)).toBe(8);
    expect(previousZoomStep(0.1)).toBe(0.1);
  });
});
