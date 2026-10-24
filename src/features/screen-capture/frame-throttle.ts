// Calls `run` with the latest value at most once per animation frame.
export type NextFrame = (callback: () => void) => unknown;

export function frameThrottle<T>(
  nextFrame: NextFrame,
  run: (value: T) => void,
): (value: T) => void {
  let pending = false;
  let latest: T;
  return (value) => {
    latest = value;
    if (pending) return;
    pending = true;
    nextFrame(() => {
      pending = false;
      run(latest);
    });
  };
}
