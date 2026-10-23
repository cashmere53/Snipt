import { describe, expect, it } from "vitest";
import { EDITOR_COMMANDS, EDITOR_ERROR_KINDS } from "./annotation-editor";

describe("annotation-editor IPC contract", () => {
  it("names the editor commands with the editor_ prefix", () => {
    const names = Object.values(EDITOR_COMMANDS);
    expect(names).toHaveLength(7);
    expect(names.every((name) => name.startsWith("editor_"))).toBe(true);
  });

  it("has the eight error kinds of the design, in its order", () => {
    expect(EDITOR_ERROR_KINDS).toEqual([
      "notFound",
      "invalidCaptureId",
      "invalidPayload",
      "library",
      "clipboard",
      "dialog",
      "io",
      "unknownToken",
    ]);
  });
});
