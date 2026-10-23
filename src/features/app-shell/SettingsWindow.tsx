// The settings window: hotkeys, fullscreen target, save folder and autostart.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type FormEvent, useState } from "react";
import type { CaptureMode, FullscreenTarget } from "@/shared/ipc/app-shell";
import { HotkeyInput } from "./HotkeyInput";
import { HotkeyStatusBanner } from "./HotkeyStatusBanner";
import { SaveDirField } from "./SaveDirField";
import { CAPTURE_MODES, MODE_LABELS, useSettingsForm } from "./use-settings-form";
import "./settings.css";

const TARGETS: { value: FullscreenTarget; label: string }[] = [
  { value: "cursorMonitor", label: "カーソルのあるモニタ" },
  { value: "allMonitors", label: "全モニタ" },
];

function closeWindow() {
  getCurrentWindow()
    .close()
    .catch((error: unknown) => console.error("close failed", error));
}

export function SettingsWindow() {
  const form = useSettingsForm();
  const [savedCount, setSavedCount] = useState(0);
  const { draft, errors } = form;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (await form.save()) setSavedCount((n) => n + 1);
  };

  return (
    <main data-testid="SettingsWindow" className="settings">
      <h1>設定</h1>
      {form.loadError && <p role="alert">{form.loadError}</p>}
      {draft && (
        <form onSubmit={(event) => void submit(event)}>
          {/* Remounted after each save so it shows the new registration state. */}
          <HotkeyStatusBanner key={savedCount} />

          <fieldset>
            <legend>ホットキー</legend>
            {CAPTURE_MODES.map((mode: CaptureMode) => (
              <HotkeyInput
                key={mode}
                id={`hotkey-${mode}`}
                label={MODE_LABELS[mode]}
                value={draft.hotkeys[mode]}
                error={errors.hotkeys[mode]}
                onChange={(accelerator) =>
                  form.edit((d) => ({ ...d, hotkeys: { ...d.hotkeys, [mode]: accelerator } }))
                }
              />
            ))}
          </fieldset>

          <fieldset>
            <legend>全画面キャプチャの対象</legend>
            {TARGETS.map((target) => (
              <label key={target.value}>
                <input
                  type="radio"
                  name="fullscreen-target"
                  value={target.value}
                  checked={draft.fullscreenTarget === target.value}
                  onChange={() => form.edit((d) => ({ ...d, fullscreenTarget: target.value }))}
                />
                {target.label}
              </label>
            ))}
          </fieldset>

          <SaveDirField
            value={draft.saveDir}
            error={errors.saveDir}
            onChange={(saveDir) => form.edit((d) => ({ ...d, saveDir }))}
          />

          <label>
            <input
              type="checkbox"
              checked={draft.autostart}
              onChange={(event) => {
                const autostart = event.currentTarget.checked;
                form.edit((d) => ({ ...d, autostart }));
              }}
            />
            Windows へのログイン時に自動で起動する
          </label>

          {errors.form && <p role="alert">{errors.form}</p>}

          <div className="actions">
            <button type="submit" disabled={!form.canSave}>
              保存
            </button>
            <button type="button" onClick={closeWindow}>
              閉じる
            </button>
          </div>
        </form>
      )}
    </main>
  );
}
