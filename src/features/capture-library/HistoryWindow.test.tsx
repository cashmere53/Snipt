import { mockConvertFileSrc } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { HistoryWindow } from "./HistoryWindow";
import { captureFixture, mockLibrary } from "./testing";

const newer = captureFixture(2);
const older = captureFixture(1);

const tile = (second: number) =>
  screen.getByRole("option", { name: new RegExp(`17:41:0${second} `) });

async function renderWindow(handlers: Parameters<typeof mockLibrary>[0] = {}) {
  const mock = mockLibrary({ library_list: () => [newer, older], ...handlers });
  render(<HistoryWindow />);
  await screen.findByRole("option", { name: /17:41:02/ });
  return mock;
}

describe("HistoryWindow", () => {
  beforeEach(() => mockConvertFileSrc("windows"));

  it("asks with the count before deleting the selection, and deletes on confirm", async () => {
    const mock = await renderWindow({
      library_delete: (args) => ({ removed: args.ids, failed: [] }),
    });

    fireEvent.click(tile(2));
    fireEvent.click(tile(1), { ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Delete" });

    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("2 件のキャプチャをごみ箱へ移動しますか?");
    expect(mock.callsTo("library_delete")).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "ごみ箱へ移動" }));
    await waitFor(() =>
      expect(mock.callsTo("library_delete")).toEqual([{ ids: [newer.id, older.id] }]),
    );
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("does not delete when cancelled", async () => {
    const mock = await renderWindow();

    fireEvent.click(tile(1));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("1 件のキャプチャ");
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));

    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(mock.callsTo("library_delete")).toEqual([]);
  });

  it("enables copy and reveal for a single selection and shows their failures", async () => {
    const mock = await renderWindow({
      library_copy: () => {
        throw { kind: "clipboard", message: "ほかのアプリがクリップボードを使用中です" };
      },
    });
    const copy = screen.getByRole("button", { name: "コピー" });
    const reveal = screen.getByRole("button", { name: "エクスプローラーで表示" });
    expect(copy).toBeDisabled();
    expect(screen.getByRole("button", { name: "削除" })).toBeDisabled();

    fireEvent.click(tile(2));
    fireEvent.click(reveal);
    expect(mock.callsTo("library_reveal")).toEqual([{ id: newer.id }]);
    fireEvent.click(copy);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "ほかのアプリがクリップボードを使用中です",
    );

    fireEvent.click(tile(1), { shiftKey: true });
    expect(copy).toBeDisabled();
    expect(screen.getByText(/2 件を選択中/)).toBeInTheDocument();
  });

  it("reports captures that could not be deleted", async () => {
    await renderWindow({
      library_delete: () => ({
        removed: [],
        failed: [{ id: older.id, error: { kind: "shell", message: "使用中です" } }],
      }),
    });

    fireEvent.click(tile(1));
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Delete" });
    fireEvent.click(screen.getByRole("button", { name: "ごみ箱へ移動" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "1 件を削除できませんでした: 使用中です",
    );
  });

  it("offers the same actions from the context menu", async () => {
    const mock = await renderWindow();

    fireEvent.contextMenu(tile(1));
    const menu = screen.getByRole("menu");
    expect(tile(1)).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByRole("menuitem", { name: "編集画面で開く" }));
    expect(mock.callsTo("library_open_in_editor")).toEqual([{ id: older.id }]);
    expect(menu).not.toBeInTheDocument();

    fireEvent.contextMenu(tile(1));
    fireEvent.click(screen.getByRole("menuitem", { name: "削除" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("1 件のキャプチャ");
  });

  it("opens a capture on double click", async () => {
    const mock = await renderWindow();
    fireEvent.doubleClick(tile(2));
    expect(mock.callsTo("library_open_in_editor")).toEqual([{ id: newer.id }]);
  });

  it("says so when there are no captures", async () => {
    mockLibrary({ library_list: () => [] });
    render(<HistoryWindow />);
    expect(await screen.findByText("キャプチャはまだありません")).toBeInTheDocument();
  });
});
