import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { Accelerator } from "@/shared/ipc/app-shell";
import { HotkeyInput } from "./HotkeyInput";
import { mockCommands } from "./testing";

function Harness() {
  const [value, setValue] = useState("PrintScreen" as Accelerator);
  return <HotkeyInput id="hk" label="範囲キャプチャ" value={value} onChange={setValue} />;
}

describe("HotkeyInput", () => {
  it("suspends hotkeys on focus and resumes on blur", () => {
    const mock = mockCommands({});
    render(<Harness />);
    const input = screen.getByLabelText("範囲キャプチャ");

    fireEvent.focus(input);
    fireEvent.blur(input);

    expect(mock.names()).toEqual(["hotkeys_suspend", "hotkeys_resume"]);
  });

  it("resumes when unmounted while focused", () => {
    const mock = mockCommands({});
    const { unmount } = render(<Harness />);
    fireEvent.focus(screen.getByLabelText("範囲キャプチャ"));

    unmount();

    expect(mock.names()).toEqual(["hotkeys_suspend", "hotkeys_resume"]);
  });

  it("shows the pressed combination", () => {
    mockCommands({});
    render(<Harness />);
    const input = screen.getByLabelText("範囲キャプチャ");
    fireEvent.focus(input);

    fireEvent.keyDown(input, { code: "ControlLeft", ctrlKey: true });
    expect(input).toHaveValue("PrintScreen");
    fireEvent.keyDown(input, { code: "KeyS", ctrlKey: true, shiftKey: true });
    expect(input).toHaveValue("Ctrl + Shift + S");
    fireEvent.keyUp(input, { code: "PrintScreen", altKey: true });
    expect(input).toHaveValue("Alt + PrintScreen");
  });

  it("asks for a modifier when an ordinary key is pressed alone", () => {
    mockCommands({});
    render(<Harness />);
    const input = screen.getByLabelText("範囲キャプチャ");

    fireEvent.keyDown(input, { code: "KeyA" });

    expect(input).toHaveValue("PrintScreen");
    expect(screen.getByRole("alert")).toHaveTextContent("組み合わせてください");
  });
});
