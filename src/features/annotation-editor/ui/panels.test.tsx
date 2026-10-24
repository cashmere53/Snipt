import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { arrow, rect } from "../model/testing";
import { currentDoc } from "../store/editor-store";
import { renderWithEditor } from "../store/testing";
import { CloseGuardDialog } from "./CloseGuardDialog";
import { NoticeBanner } from "./NoticeBanner";
import { StatusBar } from "./StatusBar";
import { StylePanel } from "./StylePanel";
import { Toolbar } from "./Toolbar";

const button = (name: string) => screen.getByRole("button", { name });

describe("Toolbar", () => {
  it("switches tools", () => {
    const f = renderWithEditor(
      <Toolbar onCopy={vi.fn<() => void>()} onSaveAs={vi.fn<() => void>()} />,
    );
    expect(button("選択")).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(button("矢印"));
    expect(f.state().tool).toBe("arrow");
    expect(button("矢印")).toHaveAttribute("aria-pressed", "true");
    expect(button("選択")).toHaveAttribute("aria-pressed", "false");
  });

  it("calls copy and save as", () => {
    const onCopy = vi.fn<() => void>();
    const onSaveAs = vi.fn<() => void>();
    renderWithEditor(<Toolbar onCopy={onCopy} onSaveAs={onSaveAs} />);
    fireEvent.click(button("コピー"));
    fireEvent.click(button("名前を付けて保存"));
    expect([onCopy.mock.calls.length, onSaveAs.mock.calls.length]).toEqual([1, 1]);
  });

  it("undoes and redoes when there is something to", () => {
    const f = renderWithEditor(
      <Toolbar onCopy={vi.fn<() => void>()} onSaveAs={vi.fn<() => void>()} />,
    );
    expect(button("元に戻す")).toBeDisabled();
    act(() => f.state().commitDoc({ ...currentDoc(f.state()), shapes: [arrow("a")] }));
    fireEvent.click(button("元に戻す"));
    expect(currentDoc(f.state()).shapes).toEqual([]);
    fireEvent.click(button("やり直す"));
    expect(currentDoc(f.state()).shapes).toEqual([arrow("a")]);
  });

  it("confirms, cancels and clears crops", () => {
    const f = renderWithEditor(
      <Toolbar onCopy={vi.fn<() => void>()} onSaveAs={vi.fn<() => void>()} />,
    );
    expect(screen.queryByRole("button", { name: "確定" })).toBeNull();
    fireEvent.click(button("切り抜き"));
    act(() => f.state().updateCropDraft({ x: 10, y: 10, width: 50, height: 50 }));
    fireEvent.click(button("確定"));
    expect(currentDoc(f.state()).crop).toEqual({ x: 10, y: 10, width: 50, height: 50 });

    fireEvent.click(button("切り抜き"));
    fireEvent.click(button("取消"));
    expect(f.state().cropDraft).toBeNull();
    fireEvent.click(button("切り抜き解除"));
    expect(currentDoc(f.state()).crop).toBeNull();
  });
});

describe("StylePanel", () => {
  it("applies the style to the selection and to the next shapes", () => {
    const f = renderWithEditor(<StylePanel />, { doc: { shapes: [rect("r")] } });
    act(() => f.state().select(["r"]));
    fireEvent.click(screen.getByRole("radio", { name: "緑" }));
    fireEvent.change(screen.getByLabelText("線の太さ"), { target: { value: "8" } });
    fireEvent.change(screen.getByLabelText("文字サイズ"), { target: { value: "48" } });
    fireEvent.click(screen.getByRole("radio", { name: "選択色" }));

    expect(currentDoc(f.state()).shapes[0]).toMatchObject({ color: "#43A047", strokeWidth: 8 });
    expect(f.state().style).toEqual({
      color: "#43A047",
      strokeWidth: 8,
      fontSize: 48,
      textBackground: "color",
    });
    expect(screen.getByRole("radio", { name: "緑" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "赤" })).toHaveAttribute("aria-checked", "false");
  });
});

describe("StatusBar", () => {
  it("shows and changes the zoom", () => {
    const f = renderWithEditor(<StatusBar onRetry={vi.fn<() => void>()} />);
    expect(screen.getByLabelText("表示倍率")).toHaveTextContent("100%");
    fireEvent.click(button("拡大"));
    expect(screen.getByLabelText("表示倍率")).toHaveTextContent("125%");
    fireEvent.click(button("縮小"));
    fireEvent.click(button("縮小"));
    expect(f.state().view.zoom).toBe(0.75);
    fireEvent.click(button("100%"));
    expect(f.state().view.zoom).toBe(1);
  });

  it("shows the save status and offers a retry after a failure", () => {
    const onRetry = vi.fn<() => void>();
    const f = renderWithEditor(<StatusBar onRetry={onRetry} />);
    expect(screen.getByText("保存済み")).toBeInTheDocument();
    act(() => f.state().saveStarted());
    expect(screen.getByText("保存中…")).toBeInTheDocument();
    act(() => f.state().saveFailed("ディスクがいっぱいです"));
    expect(screen.getByText("保存できませんでした: ディスクがいっぱいです")).toBeInTheDocument();
    fireEvent.click(button("再試行"));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});

describe("NoticeBanner", () => {
  it("lists notices and dismisses them", () => {
    const onDismiss = vi.fn<(id: number) => void>();
    render(
      <NoticeBanner
        notices={[
          { id: 1, level: "warning", message: "注釈を読み込めませんでした" },
          { id: 2, level: "error", message: "コピーできませんでした" },
        ]}
        onDismiss={onDismiss}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("コピーできませんでした");
    fireEvent.click(screen.getAllByRole("button", { name: "閉じる" })[0]!);
    expect(onDismiss).toHaveBeenCalledWith(1);
  });

  it("is empty without notices", () => {
    const { container } = render(
      <NoticeBanner notices={[]} onDismiss={vi.fn<(id: number) => void>()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});

describe("CloseGuardDialog", () => {
  it("offers retry, focused, and discard", () => {
    const onRetry = vi.fn<() => void>();
    const onDiscard = vi.fn<() => void>();
    render(<CloseGuardDialog message="書き込めません" onRetry={onRetry} onDiscard={onDiscard} />);
    expect(
      screen.getByRole("alertdialog", { name: "注釈を保存できませんでした" }),
    ).toHaveTextContent("書き込めません");
    expect(button("再試行")).toHaveFocus();
    fireEvent.click(button("再試行"));
    fireEvent.click(button("変更を破棄して閉じる"));
    expect([onRetry.mock.calls.length, onDiscard.mock.calls.length]).toEqual([1, 1]);
  });
});
