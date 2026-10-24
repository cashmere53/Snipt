// Saving before the window goes away. The user's close waits for the save and asks what to do
// when it fails; Rust's close request saves and closes whatever happens; the flush request
// before the app quits saves and answers, keeping the window.
import { useCallback, useEffect, useRef, useState } from "react";
import type { StoreApi } from "zustand/vanilla";
import type { SaveResult } from "../services/autosave";
import type { Unlisten, WindowPort } from "../services/window-port";
import type { EditorStore } from "../store/editor-store";

export interface CloseGuardDeps {
  store: StoreApi<EditorStore>;
  flush(): Promise<SaveResult>;
  port: WindowPort;
  flushResult(requestId: string, saved: boolean): Promise<unknown>;
}

export interface CloseGuard {
  /** Why the save before closing failed; the dialog is shown while it is set. */
  failure: string | null;
  retry(): void;
  discard(): void;
}

function destroy(port: WindowPort): Promise<void> {
  return port.destroy().catch((error: unknown) => {
    console.error("could not close the editor window", error);
  });
}

export function useCloseGuard({ store, flush, port, flushResult }: CloseGuardDeps): CloseGuard {
  const [failure, setFailure] = useState<string | null>(null);
  /** A close is saving or done: another close request waits for it. */
  const closing = useRef(false);

  /** The text being typed is part of what is saved. */
  const save = useCallback(() => {
    store.getState().commitTextEdit();
    return flush();
  }, [store, flush]);

  const close = useCallback(async () => {
    if (closing.current) return;
    closing.current = true;
    setFailure(null);
    const result = await save();
    if (result.ok) {
      await destroy(port);
    } else {
      closing.current = false;
      setFailure(result.message);
    }
  }, [save, port]);

  useEffect(() => {
    const subscriptions: Promise<Unlisten>[] = [
      port.onCloseRequested((event) => {
        event.preventDefault();
        void close();
      }),
      port.onCloseRequest(() => {
        closing.current = true;
        void save().then(async (result) => {
          if (!result.ok) console.error("closing the editor without saving:", result.message);
          await destroy(port);
        });
      }),
      port.onFlushRequest(({ requestId }) => {
        void save().then((result) => flushResult(requestId, result.ok));
      }),
    ];
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, [port, close, save, flushResult]);

  return {
    failure,
    retry: () => void close(),
    discard: () => void destroy(port),
  };
}
