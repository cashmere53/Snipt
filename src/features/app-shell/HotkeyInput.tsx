// A field that records a key combination. While focused, the global hotkeys are suspended so
// pressing them reaches this field instead of starting a capture.
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { type Accelerator, hotkeysResume, hotkeysSuspend } from "@/shared/ipc/app-shell";
import { acceleratorFromKeyPress, displayAccelerator } from "./keyboard-accelerator";

interface Props {
  id: string;
  label: string;
  value: Accelerator;
  error?: string;
  onChange: (accelerator: Accelerator) => void;
}

const NEEDS_MODIFIER = "Ctrl / Alt / Shift / Win のいずれかと組み合わせてください";

function logFailure(action: string) {
  return (error: unknown) => console.error(`${action} failed`, error);
}

function resumeIfSuspended(suspended: { current: boolean }) {
  if (!suspended.current) return;
  suspended.current = false;
  hotkeysResume().catch(logFailure("hotkeys_resume"));
}

export function HotkeyInput({ id, label, value, error, onChange }: Props) {
  const [hint, setHint] = useState<string | null>(null);
  const suspended = useRef(false);

  const resume = () => resumeIfSuspended(suspended);

  // Never leave the hotkeys suspended when the window or the field goes away.
  useEffect(() => () => resumeIfSuspended(suspended), []);

  const onFocus = () => {
    if (suspended.current) return;
    suspended.current = true;
    hotkeysSuspend().catch(logFailure("hotkeys_suspend"));
  };

  const onKey = (event: KeyboardEvent<HTMLInputElement>) => {
    const result = acceleratorFromKeyPress({ ...event, type: event.type });
    if (result.kind === "ignored") return;
    event.preventDefault();
    if (result.kind === "needsModifier") {
      setHint(NEEDS_MODIFIER);
      return;
    }
    setHint(null);
    onChange(result.accelerator);
  };

  const message = hint ?? error;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        readOnly
        value={displayAccelerator(value)}
        onFocus={onFocus}
        onBlur={resume}
        onKeyDown={onKey}
        onKeyUp={onKey}
        aria-invalid={message ? true : undefined}
        aria-describedby={message ? `${id}-message` : undefined}
      />
      {message && (
        <p id={`${id}-message`} role="alert" className="field-error">
          {message}
        </p>
      )}
    </div>
  );
}
