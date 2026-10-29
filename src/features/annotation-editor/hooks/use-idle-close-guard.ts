// A window with nothing to save (still loading, or failed to load) answers the requests at once:
// there is no session whose changes could be lost. Once the session opens, `useCloseGuard` answers
// them instead, so this hook is only active before that.
import { useEffect } from "react";
import type { Unlisten, WindowPort } from "../services/window-port";

export interface IdleCloseGuardDeps {
  /** True until the session mounts and takes over the requests. */
  active: boolean;
  port: WindowPort;
  flushResult(requestId: string, saved: boolean): Promise<unknown>;
}

export function useIdleCloseGuard({ active, port, flushResult }: IdleCloseGuardDeps): void {
  useEffect(() => {
    if (!active) return;
    const destroy = () => void port.destroy().catch(console.error);
    const subscriptions: Promise<Unlisten>[] = [
      port.onCloseRequested((event) => {
        event.preventDefault();
        destroy();
      }),
      port.onCloseRequest(destroy),
      port.onFlushRequest(({ requestId }) => {
        void flushResult(requestId, true).catch(console.error);
      }),
    ];
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, [active, port, flushResult]);
}
