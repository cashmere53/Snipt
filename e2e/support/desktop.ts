// Node side of desktop.ps1: one PowerShell process for the whole run, one JSON line per call.
import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { createInterface, type Interface } from "node:readline";
import { fileURLToPath } from "node:url";

const SCRIPT = fileURLToPath(new URL("./desktop.ps1", import.meta.url));

const VK: Record<string, number> = {
  Ctrl: 0x11,
  Alt: 0x12,
  Shift: 0x10,
  ...Object.fromEntries(Array.from({ length: 12 }, (_, i) => [`F${i + 1}`, 0x70 + i])),
};

export interface ClipboardImage {
  width: number;
  height: number;
  /** `[r, g, b, a]` for each requested point. */
  pixels: [number, number, number, number][];
}

type Reply = { ok: true; value: unknown } | { ok: false; error: string };

export class Desktop {
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly lines: Interface;
  private readonly waiting: ((reply: Reply) => void)[] = [];

  constructor() {
    this.child = spawn(
      "powershell.exe",
      ["-NoLogo", "-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-File", SCRIPT],
      { stdio: "pipe" },
    );
    this.child.stderr.on("data", (data: Buffer) => process.stderr.write(data));
    this.lines = createInterface({ input: this.child.stdout });
    this.lines.on("line", (line) => this.waiting.shift()?.(JSON.parse(line) as Reply));
  }

  private call<T>(command: Record<string, unknown>): Promise<T> {
    return new Promise((resolve, reject) => {
      this.waiting.push((reply) =>
        reply.ok ? resolve(reply.value as T) : reject(new Error(`desktop: ${reply.error}`)),
      );
      this.child.stdin.write(`${JSON.stringify(command)}\n`);
    });
  }

  /** Presses and releases an accelerator such as `Ctrl+Alt+Shift+F9` with real key input. */
  pressKeys(accelerator: string): Promise<boolean> {
    const vks = accelerator.split("+").map((key) => {
      const vk = VK[key];
      if (vk === undefined) {
        throw new Error(`unsupported key: ${key}`);
      }
      return vk;
    });
    return this.call({ op: "keys", vks });
  }

  foregroundTitle(): Promise<string> {
    return this.call({ op: "foreground" });
  }

  activate(title: string): Promise<boolean> {
    return this.call({ op: "activate", title });
  }

  /** Posts WM_CLOSE, as the title bar's close button does. */
  close(title: string): Promise<boolean> {
    return this.call({ op: "close", title });
  }

  windowExists(title: string): Promise<boolean> {
    return this.call({ op: "exists", title });
  }

  clearClipboard(): Promise<boolean> {
    return this.call({ op: "clipboardClear" });
  }

  /** The clipboard image (the PNG format when present), or `null` when it holds none. */
  async clipboardImage(points: readonly [number, number][] = []): Promise<ClipboardImage | null> {
    // Points go as `x,y;x,y` and pixels come back flat: PowerShell 5.1 unrolls nested arrays.
    const image = await this.call<{ width: number; height: number; pixels: number[] } | null>({
      op: "clipboardImage",
      points: points.map((p) => p.join(",")).join(";"),
    });
    if (image === null) {
      return null;
    }
    const flat = image.pixels;
    const pixels = points.map(
      (_, i) => flat.slice(i * 4, i * 4 + 4) as ClipboardImage["pixels"][0],
    );
    return { width: image.width, height: image.height, pixels };
  }

  dispose(): void {
    this.child.stdin.end();
    this.child.kill();
  }
}
