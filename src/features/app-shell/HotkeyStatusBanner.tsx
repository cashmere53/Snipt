// Explains hotkeys that could not be registered and offers to try again.
import { useEffect, useState } from "react";
import { type HotkeyStatus, hotkeysRetry, hotkeysStatus } from "@/shared/ipc/app-shell";
import { displayAccelerator } from "./keyboard-accelerator";
import { MODE_LABELS } from "./use-settings-form";

export const CONFLICT_GUIDE =
  "ほかのアプリが使用している可能性があります。別のキーを割り当ててください。";
export const RESERVED_GUIDE =
  "Windows の「設定 > アクセシビリティ > キーボード」にある「Print Screen キーを使用して画面キャプチャを開く」をオフにしてから、[再登録] を押してください。";

/** Loads the statuses when mounted; remount it (with a new `key`) to reload. */
export function HotkeyStatusBanner() {
  const [statuses, setStatuses] = useState<HotkeyStatus[]>([]);
  const [retrying, setRetrying] = useState(false);

  useEffect(() => {
    let active = true;
    hotkeysStatus()
      .then((next) => {
        if (active) setStatuses(next);
      })
      .catch((error: unknown) => console.error("hotkeys_status failed", error));
    return () => {
      active = false;
    };
  }, []);

  const failed = statuses.filter((status) => status.failure !== null);
  if (failed.length === 0) return null;

  const retry = async () => {
    setRetrying(true);
    try {
      setStatuses(await hotkeysRetry());
    } catch (error) {
      console.error("hotkeys_retry failed", error);
    } finally {
      setRetrying(false);
    }
  };

  return (
    <section role="status" className="banner" aria-label="ホットキーの登録状態">
      <ul>
        {failed.map((status) => (
          <li key={status.mode}>
            {MODE_LABELS[status.mode]}({displayAccelerator(status.accelerator)}
            )のホットキーを登録できませんでした。
            {status.failure === "printScreenReservedByWindows" ? RESERVED_GUIDE : CONFLICT_GUIDE}
          </li>
        ))}
      </ul>
      <button type="button" onClick={() => void retry()} disabled={retrying}>
        再登録
      </button>
    </section>
  );
}
