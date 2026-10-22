import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WindowRouter } from "./WindowRouter";

describe("WindowRouter", () => {
  it.each([
    ["settings", "SettingsWindow"],
    ["history", "HistoryWindow"],
    ["editor-abc", "EditorWindow"],
    ["overlay-1", "OverlayWindow"],
  ])("renders the %s window", async (label, testId) => {
    render(<WindowRouter label={label} />);
    expect(await screen.findByTestId(testId)).toBeInTheDocument();
  });

  it("hands the parsed parameters to the window", async () => {
    render(<WindowRouter label="editor-abc" />);
    expect(await screen.findByTestId("EditorWindow")).toHaveTextContent("abc");
  });

  it("renders nothing and logs for an unknown label", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const { container } = render(<WindowRouter label="main" />);
    expect(container).toBeEmptyDOMElement();
    expect(error).toHaveBeenCalledWith("Unknown window label: main");
  });
});
