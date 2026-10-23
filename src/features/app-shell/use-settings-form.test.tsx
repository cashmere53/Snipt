import { emit } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Accelerator, SettingsUpdateError } from "@/shared/ipc/app-shell";
import { SETTINGS_CHANGED_EVENT } from "@/shared/ipc/app-shell";
import { mockCommands, settingsFixture } from "./testing";
import {
  DUPLICATE_MESSAGE,
  errorsFromUpdateError,
  SAVE_DIR_NOT_WRITABLE_MESSAGE,
  useSettingsForm,
} from "./use-settings-form";

async function loadedForm(handlers: Parameters<typeof mockCommands>[0] = {}) {
  const mock = mockCommands({ settings_get: () => settingsFixture(), ...handlers });
  const hook = renderHook(() => useSettingsForm());
  await waitFor(() => expect(hook.result.current.draft).not.toBeNull());
  return { ...hook, mock };
}

describe("useSettingsForm", () => {
  it("starts from the current settings and follows changes while unedited", async () => {
    const { result } = await loadedForm();
    expect(result.current.draft).toEqual(settingsFixture());

    const changed = { ...settingsFixture(), autostart: false };
    await act(() => emit(SETTINGS_CHANGED_EVENT, changed));
    expect(result.current.draft).toEqual(changed);
  });

  it("keeps the user's edits when settings change elsewhere", async () => {
    const { result } = await loadedForm();
    act(() => result.current.edit((d) => ({ ...d, fullscreenTarget: "allMonitors" })));

    await act(() => emit(SETTINGS_CHANGED_EVENT, { ...settingsFixture(), autostart: false }));

    expect(result.current.draft?.fullscreenTarget).toBe("allMonitors");
    expect(result.current.draft?.autostart).toBe(true);
  });

  it("flags duplicate hotkeys immediately and blocks saving", async () => {
    const { result } = await loadedForm();
    act(() =>
      result.current.edit((d) => ({
        ...d,
        hotkeys: { ...d.hotkeys, fullscreen: "PrintScreen" as Accelerator },
      })),
    );

    expect(result.current.errors.hotkeys).toEqual({
      region: DUPLICATE_MESSAGE,
      fullscreen: DUPLICATE_MESSAGE,
    });
    expect(result.current.canSave).toBe(false);
    await act(async () => expect(await result.current.save()).toBe(false));
  });

  it("saves the draft and adopts the committed value", async () => {
    const committed = { ...settingsFixture(), saveDir: "D:\\Shots" };
    const { result, mock } = await loadedForm({ settings_update: () => committed });
    act(() => result.current.edit((d) => ({ ...d, saveDir: "D:\\Shots\\" })));

    await act(async () => expect(await result.current.save()).toBe(true));

    expect(mock.calls.find((c) => c.cmd === "settings_update")?.args).toEqual({
      next: { ...settingsFixture(), saveDir: "D:\\Shots\\" },
    });
    expect(result.current.draft).toEqual(committed);
  });

  it("keeps the draft and shows the error after a failed save", async () => {
    const { result } = await loadedForm({
      settings_update: () => {
        throw { kind: "persistFailed", message: "disk full" };
      },
    });
    act(() => result.current.edit((d) => ({ ...d, autostart: false })));

    await act(async () => expect(await result.current.save()).toBe(false));

    expect(result.current.draft?.autostart).toBe(false);
    expect(result.current.errors.form).toBe("設定を保存できませんでした: disk full");
  });
});

const key = (s: string) => s as Accelerator;

describe("errorsFromUpdateError", () => {
  it.each<[SettingsUpdateError, ReturnType<typeof errorsFromUpdateError>]>([
    [
      { kind: "duplicateHotkey", accelerator: key("PrintScreen"), modes: ["region", "window"] },
      { hotkeys: { region: DUPLICATE_MESSAGE, window: DUPLICATE_MESSAGE } },
    ],
    [
      { kind: "invalidHotkey", mode: "window", reason: "modifierRequired" },
      { hotkeys: { window: "Ctrl / Alt / Shift / Win のいずれかと組み合わせてください" } },
    ],
    [
      { kind: "saveDirNotWritable", path: "X:\\", message: "denied" },
      { hotkeys: {}, saveDir: SAVE_DIR_NOT_WRITABLE_MESSAGE },
    ],
    [
      {
        kind: "hotkeyRegistrationFailed",
        statuses: [
          {
            mode: "region",
            accelerator: key("PrintScreen"),
            registered: false,
            failure: "printScreenReservedByWindows",
          },
          { mode: "window", accelerator: key("Ctrl+KeyW"), registered: false, failure: null },
          {
            mode: "fullscreen",
            accelerator: key("Ctrl+KeyF"),
            registered: false,
            failure: "conflict",
          },
        ],
      },
      {
        hotkeys: {
          region: "Windows が PrintScreen キーを使用しているため登録できませんでした",
          fullscreen: "このキーは登録できませんでした。ほかのアプリが使用している可能性があります",
        },
        form: "ホットキーを登録できなかったため、変更を保存していません",
      },
    ],
    [
      { kind: "autostartFailed", message: "access denied" },
      { hotkeys: {}, form: "自動起動の設定を変更できませんでした: access denied" },
    ],
  ])("routes %o", (error, expected) => {
    expect(errorsFromUpdateError(error)).toEqual(expected);
  });
});
