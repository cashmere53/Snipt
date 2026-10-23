import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { SaveDirField } from "./SaveDirField";
import { mockCommands } from "./testing";
import { SAVE_DIR_NOT_WRITABLE_MESSAGE } from "./use-settings-form";

function Harness() {
  const [value, setValue] = useState("C:\\Pictures\\Snipt");
  return <SaveDirField value={value} onChange={setValue} />;
}

function mockDialog(picked: string | null, writable: boolean) {
  return mockCommands({
    "plugin:dialog|open": () => picked,
    settings_check_save_dir: () => {
      if (!writable) throw { kind: "saveDirNotWritable", path: picked, message: "denied" };
    },
  });
}

describe("SaveDirField", () => {
  it("uses a writable folder", async () => {
    const mock = mockDialog("D:\\Shots", true);
    render(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "変更…" }));

    await waitFor(() =>
      expect(screen.getByTestId("save-dir-value")).toHaveTextContent("D:\\Shots"),
    );
    expect(mock.calls.find((c) => c.cmd === "settings_check_save_dir")?.args).toEqual({
      path: "D:\\Shots",
    });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("rejects a folder that cannot be written", async () => {
    mockDialog("C:\\Windows", false);
    render(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "変更…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(SAVE_DIR_NOT_WRITABLE_MESSAGE);
    expect(screen.getByTestId("save-dir-value")).toHaveTextContent("C:\\Pictures\\Snipt");
  });

  it("changes nothing when the dialog is cancelled", async () => {
    const mock = mockDialog(null, true);
    render(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "変更…" }));

    await waitFor(() => expect(mock.names()).toContain("plugin:dialog|open"));
    expect(mock.names()).not.toContain("settings_check_save_dir");
    expect(screen.getByTestId("save-dir-value")).toHaveTextContent("C:\\Pictures\\Snipt");
  });
});
