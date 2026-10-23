// Turns key presses into normalized accelerators and formats them for display.
// Mirrors `app_shell::hotkeys::accelerator` in Rust.
import type { Accelerator } from "@/shared/ipc/app-shell";

/** The fields of a KeyboardEvent this module reads. */
export interface KeyPress {
  type: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

export type KeyPressResult =
  | { kind: "accelerator"; accelerator: Accelerator }
  /** A key that needs Ctrl / Alt / Shift / Win but was pressed alone. */
  | { kind: "needsModifier" }
  /** Not a complete combination yet (modifier only, or the wrong key phase). */
  | { kind: "ignored" };

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

/** Keys the browser should keep handling when pressed alone (focus moves, cancel). */
const PASSTHROUGH_ALONE = new Set(["Tab", "Escape"]);

const FUNCTION_KEY = /^F([1-9]|1[0-9]|2[0-4])$/;

/** Same rule as Rust: these keys may be used without a modifier. */
export function allowedWithoutModifier(code: string): boolean {
  return (
    code === "PrintScreen" || code === "Pause" || code === "ScrollLock" || FUNCTION_KEY.test(code)
  );
}

/**
 * Converts a keydown / keyup into an accelerator. PrintScreen is taken from keyup because
 * WebView2 does not deliver its keydown; every other key is taken from keydown.
 */
export function acceleratorFromKeyPress(press: KeyPress): KeyPressResult {
  const ignored = { kind: "ignored" } as const;
  if (MODIFIER_CODES.has(press.code) || press.code === "") return ignored;
  const wantedPhase = press.code === "PrintScreen" ? "keyup" : "keydown";
  if (press.type !== wantedPhase) return ignored;

  const modifiers: string[] = [];
  if (press.ctrlKey) modifiers.push("Ctrl");
  if (press.altKey) modifiers.push("Alt");
  if (press.shiftKey) modifiers.push("Shift");
  if (press.metaKey) modifiers.push("Win");

  if (modifiers.length === 0) {
    if (PASSTHROUGH_ALONE.has(press.code)) return ignored;
    if (!allowedWithoutModifier(press.code)) return { kind: "needsModifier" };
  }
  return { kind: "accelerator", accelerator: [...modifiers, press.code].join("+") as Accelerator };
}

function displayToken(token: string): string {
  const letter = /^Key([A-Z])$/.exec(token);
  if (letter?.[1]) return letter[1];
  const digit = /^Digit([0-9])$/.exec(token);
  if (digit?.[1]) return digit[1];
  return token;
}

/** User-facing text such as `"Ctrl + Shift + A"`, same as Rust's `accelerator::display`. */
export function displayAccelerator(accelerator: Accelerator): string {
  return accelerator.split("+").map(displayToken).join(" + ");
}
