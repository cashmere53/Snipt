// Konva draws into real canvases in the editor tests: jsdom backs <canvas> with the `canvas`
// package, so pixels can be read back.
import { render } from "@testing-library/react";
import Konva from "konva";
import { Layer, Rect, Stage } from "react-konva";
import { beforeAll, describe, expect, it } from "vitest";
import { CANVAS_WARM_UP_TIMEOUT, warmUpCanvas } from "@/test/canvas";

function pixelAt(canvas: HTMLCanvasElement, x: number, y: number): number[] {
  return [...canvas.getContext("2d")!.getImageData(x, y, 1, 1).data];
}

describe("Konva on jsdom", () => {
  beforeAll(warmUpCanvas, CANVAS_WARM_UP_TIMEOUT);

  it("draws a stage into one canvas", () => {
    const container = document.createElement("div");
    const stage = new Konva.Stage({ container, width: 4, height: 3 });
    const layer = new Konva.Layer();
    layer.add(new Konva.Rect({ x: 0, y: 0, width: 2, height: 3, fill: "#ff0000" }));
    stage.add(layer);
    layer.draw();

    const canvas = stage.toCanvas({ pixelRatio: 1 });
    expect([canvas.width, canvas.height]).toEqual([4, 3]);
    expect(pixelAt(canvas, 0, 0)).toEqual([255, 0, 0, 255]);
    expect(pixelAt(canvas, 3, 0)).toEqual([0, 0, 0, 0]);
    expect(container.querySelectorAll("canvas")).toHaveLength(1);
    stage.destroy();
  });

  it("renders react-konva components", () => {
    let stage: Konva.Stage | null = null;
    render(
      <Stage width={4} height={3} ref={(node) => void (stage = node)}>
        <Layer>
          <Rect x={2} y={0} width={2} height={3} fill="#0000ff" />
        </Layer>
      </Stage>,
    );
    const canvas = stage!.toCanvas({ pixelRatio: 1 });
    expect(pixelAt(canvas, 3, 2)).toEqual([0, 0, 255, 255]);
    expect(pixelAt(canvas, 0, 0)).toEqual([0, 0, 0, 0]);
  });
});
