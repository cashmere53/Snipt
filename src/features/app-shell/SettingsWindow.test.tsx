import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SettingsWindow } from "./SettingsWindow";
import { mockCommands, registeredStatuses, settingsFixture } from "./testing";

describe("SettingsWindow", () => {
  it("shows the current settings and saves them", async () => {
    const settings = settingsFixture();
    const mock = mockCommands({
      settings_get: () => settings,
      settings_update: (args) => args.next,
      hotkeys_status: () => [
        { ...registeredStatuses(settings)[0], registered: false, failure: "conflict" },
      ],
    });
    render(<SettingsWindow />);

    expect(await screen.findByLabelText("範囲キャプチャ")).toHaveValue("PrintScreen");
    expect(screen.getByLabelText("ウィンドウキャプチャ")).toHaveValue("Ctrl + PrintScreen");
    expect(screen.getByLabelText("全画面キャプチャ")).toHaveValue("Shift + PrintScreen");
    expect(screen.getByLabelText("カーソルのあるモニタ")).toBeChecked();
    expect(screen.getByLabelText("全モニタ")).not.toBeChecked();
    expect(screen.getByTestId("save-dir-value")).toHaveTextContent(settings.saveDir);
    expect(screen.getByLabelText("Windows へのログイン時に自動で起動する")).toBeChecked();
    expect(await screen.findByRole("status", { name: "ホットキーの登録状態" })).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("全モニタ"));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(mock.names()).toContain("settings_update"));
    expect(mock.calls.find((c) => c.cmd === "settings_update")?.args).toEqual({
      next: { ...settings, fullscreenTarget: "allMonitors" },
    });
  });

  it("disables saving while two hotkeys are the same", async () => {
    mockCommands({ settings_get: () => settingsFixture(), hotkeys_status: () => [] });
    render(<SettingsWindow />);
    const window = await screen.findByLabelText("ウィンドウキャプチャ");

    fireEvent.keyUp(window, { code: "PrintScreen" });

    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
    expect(screen.getAllByText("ほかの操作と重複しています")).toHaveLength(2);
  });
});
