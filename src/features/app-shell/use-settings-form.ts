// Draft state of the settings window: loading, following committed changes, duplicate
// detection and saving with errors routed to the right field.
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type CaptureMode,
  type HotkeySettings,
  type InvalidHotkeyReason,
  type Settings,
  SETTINGS_CHANGED_EVENT,
  settingsGet,
  settingsUpdate,
  type SettingsUpdateError,
} from "@/shared/ipc/app-shell";

export const CAPTURE_MODES: readonly CaptureMode[] = ["region", "window", "fullscreen"];

export const MODE_LABELS: Record<CaptureMode, string> = {
  region: "範囲キャプチャ",
  window: "ウィンドウキャプチャ",
  fullscreen: "全画面キャプチャ",
};

export const DUPLICATE_MESSAGE = "ほかの操作と重複しています";
export const SAVE_DIR_NOT_WRITABLE_MESSAGE = "このフォルダには保存できません";

const INVALID_MESSAGES: Record<InvalidHotkeyReason, string> = {
  noKey: "キーを指定してください",
  modifierRequired: "Ctrl / Alt / Shift / Win のいずれかと組み合わせてください",
  unknownKey: "このキーは使えません",
};

/** Where each message is shown. */
export interface FormErrors {
  hotkeys: Partial<Record<CaptureMode, string>>;
  saveDir?: string;
  form?: string;
}

const NO_ERRORS: FormErrors = { hotkeys: {} };

/** Modes whose hotkey is also used by another mode. */
export function findDuplicateModes(hotkeys: HotkeySettings): Set<CaptureMode> {
  const duplicates = new Set<CaptureMode>();
  for (const mode of CAPTURE_MODES) {
    for (const other of CAPTURE_MODES) {
      if (mode !== other && hotkeys[mode] === hotkeys[other]) duplicates.add(mode);
    }
  }
  return duplicates;
}

function isUpdateError(error: unknown): error is SettingsUpdateError {
  return typeof error === "object" && error !== null && "kind" in error;
}

/** Routes a rejected `settings_update` to the hotkey fields, the save folder or the form. */
export function errorsFromUpdateError(error: unknown): FormErrors {
  if (!isUpdateError(error)) {
    return { hotkeys: {}, form: `設定を保存できませんでした: ${String(error)}` };
  }
  switch (error.kind) {
    case "duplicateHotkey":
      return {
        hotkeys: Object.fromEntries(error.modes.map((mode) => [mode, DUPLICATE_MESSAGE])),
      };
    case "invalidHotkey":
      return { hotkeys: { [error.mode]: INVALID_MESSAGES[error.reason] } };
    case "saveDirNotWritable":
      return { hotkeys: {}, saveDir: SAVE_DIR_NOT_WRITABLE_MESSAGE };
    case "hotkeyRegistrationFailed":
      return {
        hotkeys: Object.fromEntries(
          error.statuses
            .filter((status) => status.failure !== null)
            .map((status) => [
              status.mode,
              status.failure === "printScreenReservedByWindows"
                ? "Windows が PrintScreen キーを使用しているため登録できませんでした"
                : "このキーは登録できませんでした。ほかのアプリが使用している可能性があります",
            ]),
        ),
        form: "ホットキーを登録できなかったため、変更を保存していません",
      };
    case "autostartFailed":
      return { hotkeys: {}, form: `自動起動の設定を変更できませんでした: ${error.message}` };
    case "persistFailed":
      return { hotkeys: {}, form: `設定を保存できませんでした: ${error.message}` };
  }
}

export interface SettingsForm {
  /** `null` until the current settings are loaded. */
  draft: Settings | null;
  loadError: string | null;
  errors: FormErrors;
  canSave: boolean;
  saving: boolean;
  /** Changes the draft; once edited, committed changes from elsewhere no longer replace it. */
  edit: (change: (draft: Settings) => Settings) => void;
  /** Commits the draft. Resolves `true` on success; on failure the draft is kept. */
  save: () => Promise<boolean>;
}

export function useSettingsForm(): SettingsForm {
  const [draft, setDraft] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveErrors, setSaveErrors] = useState<FormErrors>(NO_ERRORS);
  const [saving, setSaving] = useState(false);
  const dirty = useRef(false);

  useEffect(() => {
    let active = true;
    settingsGet()
      .then((settings) => {
        if (active && !dirty.current) setDraft(settings);
      })
      .catch((error: unknown) => {
        if (active) setLoadError(`設定を読み込めませんでした: ${String(error)}`);
      });
    // Unlisten synchronously when the listener is already registered.
    let unlisten: (() => void) | undefined;
    listen<Settings>(SETTINGS_CHANGED_EVENT, (event) => {
      if (!dirty.current) setDraft(event.payload);
    })
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch((error: unknown) => console.error("listen failed", error));
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const edit = useCallback((change: (draft: Settings) => Settings) => {
    dirty.current = true;
    setSaveErrors(NO_ERRORS);
    setDraft((current) => (current === null ? current : change(current)));
  }, []);

  const duplicates = useMemo(
    () => (draft === null ? new Set<CaptureMode>() : findDuplicateModes(draft.hotkeys)),
    [draft],
  );

  const errors = useMemo<FormErrors>(() => {
    const hotkeys = { ...saveErrors.hotkeys };
    for (const mode of duplicates) hotkeys[mode] = DUPLICATE_MESSAGE;
    return { ...saveErrors, hotkeys };
  }, [saveErrors, duplicates]);

  const canSave = draft !== null && duplicates.size === 0 && !saving;

  const save = useCallback(async () => {
    if (draft === null || !canSave) return false;
    setSaving(true);
    try {
      const committed = await settingsUpdate(draft);
      dirty.current = false;
      setDraft(committed);
      setSaveErrors(NO_ERRORS);
      return true;
    } catch (error) {
      setSaveErrors(errorsFromUpdateError(error));
      return false;
    } finally {
      setSaving(false);
    }
  }, [draft, canSave]);

  return { draft, loadError, errors, canSave, saving, edit, save };
}
