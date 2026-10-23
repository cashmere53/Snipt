// Save folder: picked with the folder dialog and checked for writability before it is used.
import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { settingsCheckSaveDir } from "@/shared/ipc/app-shell";
import { SAVE_DIR_NOT_WRITABLE_MESSAGE } from "./use-settings-form";

interface Props {
  value: string;
  error?: string;
  onChange: (path: string) => void;
}

export function SaveDirField({ value, error, onChange }: Props) {
  const [checkError, setCheckError] = useState<string | null>(null);

  const choose = async () => {
    const picked = await open({ directory: true, multiple: false, defaultPath: value });
    if (typeof picked !== "string") return;
    try {
      await settingsCheckSaveDir(picked);
      setCheckError(null);
      onChange(picked);
    } catch {
      setCheckError(SAVE_DIR_NOT_WRITABLE_MESSAGE);
    }
  };

  const message = checkError ?? error;
  return (
    <div className="field">
      <span id="save-dir-label">保存先フォルダ</span>
      <div className="save-dir">
        <output aria-labelledby="save-dir-label" data-testid="save-dir-value">
          {value}
        </output>
        <button type="button" onClick={() => void choose()}>
          変更…
        </button>
      </div>
      {message && (
        <p role="alert" className="field-error">
          {message}
        </p>
      )}
    </div>
  );
}
