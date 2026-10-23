import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { HotkeyStatus } from "@/shared/ipc/app-shell";
import { CONFLICT_GUIDE, HotkeyStatusBanner, RESERVED_GUIDE } from "./HotkeyStatusBanner";
import { mockCommands, registeredStatuses, settingsFixture } from "./testing";

function withFailures(): HotkeyStatus[] {
  const [region, window, fullscreen] = registeredStatuses(settingsFixture()) as [
    HotkeyStatus,
    HotkeyStatus,
    HotkeyStatus,
  ];
  return [
    { ...region, registered: false, failure: "printScreenReservedByWindows" },
    { ...window, registered: false, failure: "conflict" },
    fullscreen,
  ];
}

describe("HotkeyStatusBanner", () => {
  it("renders nothing when every hotkey is registered", async () => {
    const mock = mockCommands({ hotkeys_status: () => registeredStatuses(settingsFixture()) });
    const { container } = render(<HotkeyStatusBanner />);
    await waitFor(() => expect(mock.names()).toContain("hotkeys_status"));
    expect(container).toBeEmptyDOMElement();
  });

  it("explains conflicts and the Windows PrintScreen reservation", async () => {
    mockCommands({ hotkeys_status: withFailures });
    render(<HotkeyStatusBanner />);

    const items = await screen.findAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent(
      "範囲キャプチャ(PrintScreen)のホットキーを登録できませんでした。",
    );
    expect(items[0]).toHaveTextContent(RESERVED_GUIDE);
    expect(items[1]).toHaveTextContent("ウィンドウキャプチャ(Ctrl + PrintScreen)");
    expect(items[1]).toHaveTextContent(CONFLICT_GUIDE);
  });

  it("drops the guidance for hotkeys that register on retry", async () => {
    const [region, window, fullscreen] = withFailures() as [
      HotkeyStatus,
      HotkeyStatus,
      HotkeyStatus,
    ];
    mockCommands({
      hotkeys_status: withFailures,
      hotkeys_retry: () => [{ ...region, registered: true, failure: null }, window, fullscreen],
    });
    render(<HotkeyStatusBanner />);
    fireEvent.click(await screen.findByRole("button", { name: "再登録" }));

    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(1));
    expect(screen.getByRole("listitem")).toHaveTextContent("ウィンドウキャプチャ");
  });
});
