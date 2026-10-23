import { describe, expect, it } from "vitest";
import type { Accelerator } from "@/shared/ipc/app-shell";
import { acceleratorFromKeyPress, displayAccelerator, type KeyPress } from "./keyboard-accelerator";

function press(type: "keydown" | "keyup", code: string, mods: Partial<KeyPress> = {}): KeyPress {
  return { type, code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods };
}

describe("acceleratorFromKeyPress", () => {
  it("builds Ctrl+Shift+KeyA in normalized modifier order", () => {
    expect(
      acceleratorFromKeyPress(press("keydown", "KeyA", { shiftKey: true, ctrlKey: true })),
    ).toEqual({
      kind: "accelerator",
      accelerator: "Ctrl+Shift+KeyA",
    });
  });

  it("takes PrintScreen from keyup only", () => {
    expect(acceleratorFromKeyPress(press("keydown", "PrintScreen")).kind).toBe("ignored");
    expect(acceleratorFromKeyPress(press("keyup", "PrintScreen"))).toEqual({
      kind: "accelerator",
      accelerator: "PrintScreen",
    });
    expect(acceleratorFromKeyPress(press("keyup", "PrintScreen", { ctrlKey: true }))).toEqual({
      kind: "accelerator",
      accelerator: "Ctrl+PrintScreen",
    });
  });

  it("does not confirm a modifier pressed alone", () => {
    expect(acceleratorFromKeyPress(press("keydown", "ControlLeft", { ctrlKey: true })).kind).toBe(
      "ignored",
    );
    expect(acceleratorFromKeyPress(press("keydown", "ShiftRight", { shiftKey: true })).kind).toBe(
      "ignored",
    );
    expect(acceleratorFromKeyPress(press("keydown", "MetaLeft", { metaKey: true })).kind).toBe(
      "ignored",
    );
  });

  it("asks for a modifier for ordinary keys but allows function keys alone", () => {
    expect(acceleratorFromKeyPress(press("keydown", "KeyA")).kind).toBe("needsModifier");
    expect(acceleratorFromKeyPress(press("keydown", "F13"))).toEqual({
      kind: "accelerator",
      accelerator: "F13",
    });
    expect(acceleratorFromKeyPress(press("keydown", "Tab")).kind).toBe("ignored");
    expect(acceleratorFromKeyPress(press("keyup", "KeyA", { ctrlKey: true })).kind).toBe("ignored");
  });
});

describe("displayAccelerator", () => {
  it.each([
    ["PrintScreen", "PrintScreen"],
    ["Ctrl+PrintScreen", "Ctrl + PrintScreen"],
    ["Ctrl+Shift+KeyA", "Ctrl + Shift + A"],
    ["Alt+Digit1", "Alt + 1"],
  ])("formats %s as %s", (accelerator, text) => {
    expect(displayAccelerator(accelerator as Accelerator)).toBe(text);
  });
});
